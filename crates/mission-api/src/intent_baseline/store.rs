use super::*;

fn read<T: serde::de::DeserializeOwned>(root: &Path, path: &Path) -> Result<T, MissionError> {
    typed_source::ensure_safe_path(root, path)?;
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes)
        .map_err(|e| intent_error("intent-record-corrupt", format!("{}: {e}", path.display())))
}

fn safe_optional_path(root: &Path, path: &Path) -> Result<(), MissionError> {
    let relative = path.strip_prefix(root).map_err(|_| {
        intent_error(
            "intent-path-invalid",
            "Intent paths must remain in the store.",
        )
    })?;
    let mut current = root.to_path_buf();
    typed_source::ensure_safe_path(root, root)?;
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err(intent_error(
                "intent-path-invalid",
                "Invalid managed path component.",
            ));
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(intent_error(
                    "intent-path-invalid",
                    "Intent paths must not contain symlinks.",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn immutable<T: Serialize>(root: &Path, path: &Path, record: &T) -> Result<(), MissionError> {
    safe_optional_path(root, path)?;
    let parent = path
        .parent()
        .ok_or_else(|| intent_error("intent-path-invalid", "No parent."))?;
    let bytes = serde_json::to_vec(record)?;
    // Staging is outside the committed chain, so interruption cannot add a false amendment.
    let temporary = root.join(format!(".{}.intent-write.tmp", Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    use std::io::Write;
    let result = (|| -> Result<(), MissionError> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::create_dir_all(parent)?;
        safe_optional_path(root, path)?;
        fs::hard_link(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    match (result, fs::remove_file(&temporary)) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(cleanup)) => Err(intent_error(
            "intent-write-failed",
            format!("{error}; staging cleanup also failed: {cleanup}"),
        )),
    }
}

impl MissionStore {
    pub fn proposed_intent_baseline(
        &self,
        id: Uuid,
        expected_revision: u64,
        requirements: &[BaselineRequirementV4],
    ) -> Result<(String, String), MissionError> {
        let _lock = typed_source::TypedLock::acquire_shared_existing(&self.root, id)?;
        if self.intent_revision(id)? != expected_revision {
            return Err(intent_error(
                "intent-revision-stale",
                "Expected current Mission revision.",
            ));
        }
        let path = self
            .root
            .join(id.to_string())
            .join("input/submitted-input.bin");
        typed_source::ensure_safe_path(&self.root, &path)?;
        let bytes = fs::read(path)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| intent_error("intent-source-invalid", "Original source must be UTF-8."))?;
        validate_requirements(requirements, source)?;
        Ok((
            intent_source_digest(&bytes),
            intent_baseline_digest(id, requirements)?,
        ))
    }

    pub fn proposed_intent_amendment(
        &self,
        id: Uuid,
        request: &IntentAmendmentRequest,
        provider: &impl IntentReceiptProvider,
    ) -> Result<(String, String), MissionError> {
        let _lock = typed_source::TypedLock::acquire_shared_existing(&self.root, id)?;
        if self.intent_revision(id)? != request.expected_revision {
            return Err(intent_error(
                "intent-revision-stale",
                "Expected current Mission revision.",
            ));
        }
        let mut effective = self.read_intent(id, provider)?.ok_or_else(|| {
            intent_error(
                "intent-confirmation-required",
                "Confirm baseline before amendments.",
            )
        })?;
        if request.amendment_id.trim().is_empty()
            || request.source.trim().is_empty()
            || request.changes.is_empty()
            || request.prior_chain_sha256 != effective.chain_sha256
            || effective
                .amendments
                .iter()
                .any(|a| a.amendment_id == request.amendment_id)
        {
            return Err(intent_error(
                "intent-conflict",
                "Expected a new amendment on the current intent chain.",
            ));
        }
        apply_changes(&mut effective, &request.source, &request.changes)?;
        Ok((
            intent_source_digest(request.source.as_bytes()),
            intent_amendment_digest(
                id,
                &request.amendment_id,
                &request.prior_chain_sha256,
                &request.source,
                &request.changes,
            )?,
        ))
    }

    fn intent_revision(&self, id: Uuid) -> Result<u64, MissionError> {
        let source = self.root.join(id.to_string()).join("source.json");
        safe_optional_path(&self.root, &source)?;
        if source.try_exists()? {
            let lifecycle =
                super::MissionLifecycle::open(self.root.ancestors().nth(3).unwrap_or(&self.root));
            Ok(lifecycle.get_mission_source(id)?.revision)
        } else {
            Ok(self.get(id)?.bundle.manifest.revision)
        }
    }

    pub fn confirm_intent_baseline(
        &self,
        id: Uuid,
        expected_revision: u64,
        mut requirements: Vec<BaselineRequirementV4>,
        reference: IntentReceiptReference,
        provider: &impl IntentReceiptProvider,
    ) -> Result<IntentBaselineV4, MissionError> {
        let _lock = typed_source::TypedLock::acquire(&self.root, id)?;
        if self.intent_revision(id)? != expected_revision {
            return Err(intent_error(
                "intent-revision-stale",
                "Expected current Mission revision.",
            ));
        }
        let directory = self.root.join(id.to_string());
        let source_path = directory.join("input/submitted-input.bin");
        typed_source::ensure_safe_path(&self.root, &source_path)?;
        let bytes = fs::read(source_path)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| intent_error("intent-source-invalid", "Original source must be UTF-8."))?;
        validate_requirements(&requirements, source)?;
        requirements.sort_by(|a, b| a.requirement_id.cmp(&b.requirement_id));
        let sha = intent_baseline_digest(id, &requirements)?;
        let source_sha = intent_source_digest(&bytes);
        let receipt = receipt(provider, id, &reference, &source_sha, &sha)?;
        let record = IntentBaselineV4 {
            schema_version: INTENT_SCHEMA_VERSION,
            mission_id: id,
            source_sha256: source_sha,
            requirements,
            baseline_sha256: sha,
            receipt_ref: reference,
            receipt,
        };
        let path = directory.join("intent/baseline.json");
        safe_optional_path(&self.root, &path)?;
        if path.try_exists()? {
            let existing: IntentBaselineV4 = read(&self.root, &path)?;
            if existing == record {
                return Ok(existing);
            }
            return Err(intent_error(
                "intent-conflict",
                "Confirmed baseline cannot be replaced.",
            ));
        }
        immutable(&self.root, &path, &record)?;
        Ok(record)
    }

    pub fn effective_intent(
        &self,
        id: Uuid,
        provider: &impl IntentReceiptProvider,
    ) -> Result<Option<EffectiveIntentV4>, MissionError> {
        let _lock = typed_source::TypedLock::acquire_shared_existing(&self.root, id)?;
        self.intent_revision(id)?;
        self.read_intent(id, provider)
    }

    fn read_intent(
        &self,
        id: Uuid,
        provider: &impl IntentReceiptProvider,
    ) -> Result<Option<EffectiveIntentV4>, MissionError> {
        let directory = self.root.join(id.to_string());
        let path = directory.join("intent/baseline.json");
        safe_optional_path(&self.root, &path)?;
        if !path.try_exists()? {
            if directory.join("intent").try_exists()? {
                if fs::read_dir(directory.join("intent"))?.next().is_none() {
                    return Err(intent_error(
                        "intent-write-pending",
                        "Uncommitted baseline directory; retry the same confirmed baseline.",
                    ));
                }
                return Err(intent_error(
                    "intent-record-corrupt",
                    "Intent directory without a baseline.",
                ));
            }
            return Ok(None);
        }
        let baseline: IntentBaselineV4 = read(&self.root, &path)?;
        let source_path = directory.join("input/submitted-input.bin");
        typed_source::ensure_safe_path(&self.root, &source_path)?;
        let bytes = fs::read(source_path)?;
        let source = std::str::from_utf8(&bytes)
            .map_err(|_| intent_error("intent-source-invalid", "Original source must be UTF-8."))?;
        validate_requirements(&baseline.requirements, source)?;
        if baseline.schema_version != INTENT_SCHEMA_VERSION
            || baseline.mission_id != id
            || baseline.source_sha256 != intent_source_digest(&bytes)
            || baseline.baseline_sha256 != intent_baseline_digest(id, &baseline.requirements)?
            || baseline.receipt
                != receipt(
                    provider,
                    id,
                    &baseline.receipt_ref,
                    &baseline.source_sha256,
                    &baseline.baseline_sha256,
                )?
        {
            return Err(intent_error(
                "intent-record-corrupt",
                "Baseline source, identity or receipt changed.",
            ));
        }
        let mut effective = EffectiveIntentV4 {
            requirements: baseline.requirements.clone(),
            chain_sha256: digest("mission-intent-chain/v4", &baseline)?,
            baseline,
            amendments: vec![],
            excluded_requirement_ids: vec![],
        };
        let amendments = directory.join("intent/amendments");
        safe_optional_path(&self.root, &amendments)?;
        if amendments.try_exists()? {
            let mut paths = fs::read_dir(&amendments)?
                .map(|entry| entry.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()?;
            paths.sort();
            for (index, path) in paths.iter().enumerate() {
                if path.file_name().and_then(|s| s.to_str())
                    != Some(format!("{:08}.json", index + 1).as_str())
                {
                    return Err(intent_error(
                        "intent-record-corrupt",
                        "Amendment sequence is not contiguous.",
                    ));
                }
                let amendment: IntentAmendmentV4 = read(&self.root, path)?;
                apply_amendment(&mut effective, amendment, provider)?;
            }
        }
        Ok(Some(effective))
    }

    pub fn append_intent_amendment(
        &self,
        id: Uuid,
        request: IntentAmendmentRequest,
        provider: &impl IntentReceiptProvider,
    ) -> Result<IntentAmendmentV4, MissionError> {
        let IntentAmendmentRequest {
            expected_revision,
            amendment_id,
            prior_chain_sha256: prior_chain,
            source,
            changes,
            reference,
        } = request;
        let _lock = typed_source::TypedLock::acquire(&self.root, id)?;
        if self.intent_revision(id)? != expected_revision {
            return Err(intent_error(
                "intent-revision-stale",
                "Expected current Mission revision.",
            ));
        }
        let mut effective = self.read_intent(id, provider)?.ok_or_else(|| {
            intent_error(
                "intent-confirmation-required",
                "Confirm baseline before amendments.",
            )
        })?;
        let sha = intent_amendment_digest(id, &amendment_id, &prior_chain, &source, &changes)?;
        let receipt = receipt(
            provider,
            id,
            &reference,
            &intent_source_digest(source.as_bytes()),
            &sha,
        )?;
        let sequence = effective
            .amendments
            .iter()
            .find(|a| a.amendment_id == amendment_id)
            .map_or(effective.amendments.len() as u64 + 1, |a| a.sequence);
        let amendment = IntentAmendmentV4 {
            schema_version: INTENT_SCHEMA_VERSION,
            amendment_id,
            sequence,
            prior_chain_sha256: prior_chain,
            source,
            changes,
            amendment_sha256: sha,
            receipt_ref: reference,
            receipt,
        };
        if let Some(existing) = effective
            .amendments
            .iter()
            .find(|a| a.amendment_id == amendment.amendment_id)
        {
            if existing == &amendment {
                return Ok(existing.clone());
            }
            return Err(intent_error(
                "intent-conflict",
                "Amendment ID reused with different contents.",
            ));
        }
        apply_amendment(&mut effective, amendment.clone(), provider)?;
        immutable(
            &self.root,
            &self
                .root
                .join(id.to_string())
                .join(format!("intent/amendments/{sequence:08}.json")),
            &amendment,
        )?;
        Ok(amendment)
    }
}
