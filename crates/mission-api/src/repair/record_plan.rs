use super::*;

impl MissionLifecycle {
    pub(super) fn plan_repair_record(
        &self,
        source_root: &Path,
        request: &RepairRecordRequest,
    ) -> (RepairRecordResponse, Option<RepairCandidate>) {
        let mission_dir = self.store.root.join(request.mission_id.to_string());
        let root_ok = mission_dir.is_dir() && !contains_symlink(&mission_dir);
        let mut response = RepairRecordResponse {
            mission_id: request.mission_id,
            readiness: "blocked".into(),
            blockers: Vec::new(),
            disposition: None,
            observed_revision: None,
            observed_accepted_sha256: None,
            observed_source_sha256: None,
            payload_fingerprint: None,
            root_ok,
            reference_ok: false,
            provenance: None,
            predicted_revision: None,
        };
        if !root_ok {
            response
                .blockers
                .push("mission-root-missing-or-unsafe".into());
            return (response, None);
        }
        let accepted_path = mission_dir.join("accepted.json");
        let accepted_bytes = match fs::read(&accepted_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-read-failed: {error}"));
                return (response, None);
            }
        };
        response.observed_accepted_sha256 = Some(sha256(&accepted_bytes));
        let source_path = mission_dir.join("source.json");
        let source_bytes = match fs::read(&source_path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                response
                    .blockers
                    .push(format!("source-read-failed: {error}"));
                return (response, None);
            }
        };
        response.observed_source_sha256 = Some(
            source_bytes
                .as_deref()
                .map(sha256)
                .unwrap_or_else(|| "missing".into()),
        );
        let files = match fingerprint_files(&mission_dir) {
            Ok(files) => files,
            Err(error) => {
                response
                    .blockers
                    .push(format!("payload-fingerprint-failed: {error}"));
                return (response, None);
            }
        };
        let fingerprint = payload_fingerprint(&files);
        response.payload_fingerprint = Some(fingerprint.clone());
        let accepted: AcceptedMission = match serde_json::from_slice(&accepted_bytes) {
            Ok(accepted) => accepted,
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-json-invalid: {error}"));
                return (response, None);
            }
        };
        response.observed_revision = Some(accepted.bundle.manifest.revision);
        response.provenance = Some(if source_bytes.is_some() {
            "typed-source-present".into()
        } else {
            "converted-from-accepted-history".into()
        });
        if accepted.mission_id != request.mission_id {
            response.blockers.push("accepted-identity-mismatch".into());
        }
        let has_source = source_bytes.is_some();
        let mut source = match source_bytes {
            Some(_) => match self.get_mission_source_unchecked(request.mission_id) {
                Ok(source) => source,
                Err(error) => {
                    response
                        .blockers
                        .push(format!("typed-source-invalid: {error}"));
                    return (response, None);
                }
            },
            None => match typed_migration::convert(source_root, &accepted) {
                Ok(source) => source,
                Err(error) => {
                    response
                        .blockers
                        .push(format!("typed-conversion-blocked: {error}"));
                    return (response, None);
                }
            },
        };
        let ownership_source = match typed_migration::convert(source_root, &accepted) {
            Ok(source) => source,
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-reference-invalid: {error}"));
                return (response, None);
            }
        };
        response.reference_ok = merge_accepted_ownership(&mut source, &ownership_source);
        if !response.reference_ok {
            response
                .blockers
                .push("typed-waypoint-identity-does-not-match-accepted-record".into());
        }
        if let Err(error) = validate_bundle(&accepted.bundle) {
            response
                .blockers
                .push(format!("accepted-bundle-invalid: {}", error.len()));
        }
        if let Err(error) = validate_typed(&source, false) {
            response
                .blockers
                .push(format!("typed-source-invalid: {error}"));
        }
        let original_source_revision = source.revision;
        let Some(next_revision) = accepted.bundle.manifest.revision.checked_add(1) else {
            response.blockers.push("revision-exhausted".into());
            return (response, None);
        };
        response.predicted_revision = Some(next_revision);
        source.revision = next_revision;
        let next = match accepted_from_typed(&source, Some(&accepted)) {
            Ok(next) => next,
            Err(error) => {
                response
                    .blockers
                    .push(format!("typed-merge-failed: {error}"));
                return (response, None);
            }
        };
        if let Err(errors) = validate_bundle(&next.bundle) {
            response
                .blockers
                .push(format!("repaired-bundle-invalid: {}", errors.len()));
        }
        if let Err(errors) = self
            .store
            .validate_child_missions(request.mission_id, &next.bundle)
        {
            response
                .blockers
                .push(format!("child-reference-invalid: {}", errors.len()));
        }
        let history = mission_dir.join("history");
        if !history.is_dir() {
            response.blockers.push("history-directory-missing".into());
        }
        for name in [
            format!("revision-{next_revision}.json"),
            format!("source-revision-{next_revision}.json"),
        ] {
            if history.join(name).exists() {
                response
                    .blockers
                    .push("next-history-revision-already-exists".into());
            }
        }
        if request.expected_revision != accepted.bundle.manifest.revision {
            response.blockers.push("expected-revision-mismatch".into());
        }
        if !matches_hash(&request.accepted_sha256, &response.observed_accepted_sha256) {
            response.blockers.push("accepted-sha256-mismatch".into());
        }
        if !matches_hash(&request.source_sha256, &response.observed_source_sha256) {
            response.blockers.push("source-sha256-mismatch".into());
        }
        if request.payload_fingerprint.is_empty() {
            response
                .blockers
                .push("payload-fingerprint-required".into());
        } else if request.payload_fingerprint != fingerprint {
            response
                .blockers
                .push("payload-fingerprint-mismatch".into());
        }
        if !response.blockers.is_empty() {
            return (response, None);
        }
        let current_is_conformant = has_source
            && original_source_revision == accepted.bundle.manifest.revision
            && accepted_from_typed(
                &TypedDraft {
                    revision: accepted.bundle.manifest.revision,
                    ..source.clone()
                },
                Some(&accepted),
            )
            .is_ok_and(|candidate| candidate == accepted);
        if current_is_conformant {
            response.readiness = "ready".into();
            response.disposition = Some("already_conformant".into());
            return (response, None);
        }
        let rendered = match self
            .store
            .render_projection(&next.bundle, request.mission_id, None)
        {
            Ok(rendered) => rendered,
            Err(errors) => {
                response.blockers.push(format!("render-failed: {errors}"));
                return (response, None);
            }
        };
        let projected = match projected_fingerprint(
            &files,
            &next,
            &source,
            &rendered,
            &MissionLifecycle {
                store: MissionStore::open(source_root),
            },
        ) {
            Ok(projected) => projected,
            Err(error) => {
                response
                    .blockers
                    .push(format!("projection-failed: {error}"));
                return (response, None);
            }
        };
        let predicted_accepted_sha256 = match serde_json::to_vec_pretty(&next) {
            Ok(bytes) => sha256(&bytes),
            Err(error) => {
                response
                    .blockers
                    .push(format!("accepted-serialization-failed: {error}"));
                return (response, None);
            }
        };
        let predicted_source_sha256 = match serde_json::to_vec_pretty(&stored_source(&source)) {
            Ok(bytes) => sha256(&bytes),
            Err(error) => {
                response
                    .blockers
                    .push(format!("source-serialization-failed: {error}"));
                return (response, None);
            }
        };
        let candidate = RepairCandidate {
            source_root: source_root.to_path_buf(),
            mission_id: request.mission_id,
            observed_revision: accepted.bundle.manifest.revision,
            observed_accepted_sha256: response
                .observed_accepted_sha256
                .clone()
                .unwrap_or_default(),
            observed_source_sha256: response.observed_source_sha256.clone().unwrap_or_default(),
            observed_payload_fingerprint: fingerprint,
            predicted_revision: next_revision,
            predicted_accepted_sha256,
            predicted_source_sha256,
            predicted_payload_fingerprint: payload_fingerprint(&projected),
            typed_source: source,
            accepted: next,
            rendered,
        };
        response.readiness = "ready".into();
        response.disposition = Some("repair".into());
        (response, Some(candidate))
    }
}
