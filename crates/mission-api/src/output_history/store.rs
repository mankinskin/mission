use super::*;

impl MissionStore {
    /// Binding for the exact current JSON bytes; not a cryptographic signature.
    pub fn output_contract_binding(&self, mission_id: Uuid) -> Result<String, MissionError> {
        self.output_contract(mission_id)?;
        Ok(format!(
            "sha256:{}",
            digest(&fs::read(
                self.root
                    .join(mission_id.to_string())
                    .join("output-contract.json"),
            )?)
        ))
    }

    /// Read and validate all bytes of the current output contract.
    pub fn output_contract(&self, mission_id: Uuid) -> Result<MissionOutputContract, MissionError> {
        typed_source::ensure_safe_path(&self.root, &self.root.join(mission_id.to_string()))?;
        let accepted = self.get(mission_id)?;
        validate_contract(
            &self.root.join(mission_id.to_string()),
            mission_id,
            accepted.bundle.manifest.revision,
        )
    }

    /// Validate current and every available archived state; never repair or fabricate.
    pub fn verify_output_history(&self, mission_id: Uuid) -> Result<(), MissionError> {
        let directory = self.root.join(mission_id.to_string());
        let current = self.output_contract(mission_id)?;
        let _ = history(Some(&directory), current.revision.saturating_add(1))?;
        let archive = directory
            .join("history/states")
            .join(&current.revision_label);
        validate_contract(&archive, mission_id, current.revision)?;
        if fs::read(archive.join("output-contract.json"))?
            != fs::read(directory.join("output-contract.json"))?
        {
            return Err(invalid("Current and archived contracts differ."));
        }
        self.validate_preserved_events(mission_id)?;
        Ok(())
    }

    pub(crate) fn validate_preserved_events(&self, mission_id: Uuid) -> Result<(), MissionError> {
        let directory = self.root.join(mission_id.to_string());
        let events = directory.join("history/events");
        if events.exists() {
            typed_source::ensure_safe_path(&directory, &events)?;
            for entry in fs::read_dir(events)? {
                let entry = entry?;
                typed_source::ensure_safe_path(&directory, &entry.path())?;
                let event: MissionApprovalEvent = serde_json::from_slice(&fs::read(entry.path())?)?;
                if entry.file_name().to_str()
                    != Some(format!("revision-{}.json", event.reaction.revision).as_str())
                {
                    return Err(invalid("Reaction path identity mismatch."));
                }
                self.validate_event(&event, &directory)?;
            }
        }
        Ok(())
    }

    fn validate_event(
        &self,
        event: &MissionApprovalEvent,
        directory: &Path,
    ) -> Result<(), MissionError> {
        if event.schema_version != 1
            || event.event_id.is_nil()
            || event.mission_id != super::history::self_id(directory)?
            || event.recorded_at_unix_ms == 0
            || event.reaction.actor.trim().is_empty()
            || event.reaction.actor_source.trim().is_empty()
        {
            return Err(invalid(
                "Reaction identity, actor and recording metadata are required.",
            ));
        }
        let archive = directory
            .join("history/states")
            .join(format!("revision-{}", event.reaction.revision));
        typed_source::ensure_safe_path(directory, &archive)?;
        validate_contract(&archive, event.mission_id, event.reaction.revision)?;
        if event.reaction.signature
            != format!(
                "sha256:{}",
                digest(&fs::read(archive.join("output-contract.json"))?)
            )
        {
            return Err(invalid("Reaction contract binding mismatch."));
        }
        Ok(())
    }

    /// Strict JSON input; one create-new reaction per current revision.
    /// Recording a reaction never changes Mission authority or grants execution.
    pub fn record_approval_event_json(
        &self,
        mission_id: Uuid,
        json: &[u8],
    ) -> Result<MissionApprovalEvent, MissionError> {
        let request: ApprovalEventRequest = serde_json::from_slice(json)?;
        self.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.root, mission_id)?;
        let contract = self.output_contract(mission_id)?;
        let directory = self.root.join(mission_id.to_string());
        self.verify_output_history(mission_id)?;
        if request.revision != contract.revision {
            return Err(invalid(
                "Reaction must bind to the current revision; historical approval reuse is forbidden.",
            ));
        }
        let (event_id, recorded_at_unix_ms) = self.event_metadata()?;
        let event = MissionApprovalEvent {
            schema_version: 1,
            event_id,
            mission_id,
            recorded_at_unix_ms,
            reaction: request,
        };
        self.validate_event(&event, &directory)?;
        let events = directory.join("history/events");
        fs::create_dir_all(&events)?;
        typed_source::ensure_safe_path(&directory, &events)?;
        let destination = events.join(format!("revision-{}.json", contract.revision));
        let pending = events.join(format!(".{}.pending", event.event_id));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&pending)?;
            file.write_all(&serde_json::to_vec_pretty(&event)?)?;
            file.sync_all()?;
            // Link is atomic and create-new, unlike rename which can replace a decision.
            fs::hard_link(&pending, destination)?;
            Ok::<_, MissionError>(())
        })();
        if pending.exists() {
            fs::remove_file(pending)?;
        }
        result?;
        fs::File::open(events)?.sync_all()?;
        Ok(event)
    }

    /// Read a recorded reaction and validate its exact immutable contract binding.
    pub fn approval_event(
        &self,
        mission_id: Uuid,
        revision: u64,
    ) -> Result<MissionApprovalEvent, MissionError> {
        let directory = self.root.join(mission_id.to_string());
        let path = directory
            .join("history/events")
            .join(format!("revision-{revision}.json"));
        typed_source::ensure_safe_path(&self.root, &path)?;
        let event: MissionApprovalEvent = serde_json::from_slice(&fs::read(path)?)?;
        if event.reaction.revision != revision || event.mission_id != mission_id {
            return Err(invalid("Reaction identity does not match its path."));
        }
        self.validate_event(&event, &directory)?;
        Ok(event)
    }
}
