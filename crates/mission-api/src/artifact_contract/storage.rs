use super::*;

pub(crate) fn validate_file_outputs(
    provenance: &MissionArtifactProvenance,
    mission_directory: &Path,
    immutable_only: bool,
) -> Result<(), MissionError> {
    let root = mission_directory.canonicalize()?;
    for file in &provenance.files {
        if immutable_only && !file.immutable {
            continue;
        }
        let path = mission_directory.join(&file.path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(validation_error(
                    "mission-artifact-missing",
                    &path,
                    "A declared immutable output is missing.",
                ));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(validation_error(
                "mission-artifact-destination-invalid",
                &path,
                "Declared outputs must be regular files.",
            ));
        }
        let canonical = path.canonicalize()?;
        if !canonical.starts_with(&root) {
            return Err(validation_error(
                "mission-artifact-destination-invalid",
                &path,
                "Declared outputs must remain inside their Mission directory.",
            ));
        }
        let bytes = fs::read(&path)?;
        match file.format {
            MissionFileFormat::Json => {
                super::provenance::validate_json_output(provenance, &file.path, &bytes)?;
            }
            MissionFileFormat::Markdown => {
                if std::str::from_utf8(&bytes).is_err() {
                    return Err(validation_error(
                        "mission-artifact-format-invalid",
                        &path,
                        "Markdown outputs must be valid UTF-8.",
                    ));
                }
            }
        }
    }
    Ok(())
}

impl MissionStore {
    pub fn get_artifact_provenance(
        &self,
        mission_id: Uuid,
        revision: u64,
    ) -> Result<MissionArtifactProvenance, MissionError> {
        let path = self
            .root
            .join(mission_id.to_string())
            .join("history")
            .join(format!("provenance-revision-{revision}.json"));
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(MissionError::NotFound(mission_id));
            }
            Err(error) => return Err(error.into()),
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(validation_error(
                "mission-provenance-destination-invalid",
                &path,
                "Provenance records must be regular files.",
            ));
        }
        let provenance: MissionArtifactProvenance = serde_json::from_slice(&fs::read(path)?)?;
        if provenance.mission_id != mission_id || provenance.revision != revision {
            return Err(validation_error(
                "mission-provenance-identity-mismatch",
                &self.root,
                "Provenance identity must match its Mission and revision path.",
            ));
        }
        provenance
            .validate()
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        let mission_directory = self.root.join(mission_id.to_string());
        validate_file_outputs(&provenance, &mission_directory, true)?;
        Ok(provenance)
    }

    pub(crate) fn validate_artifact_references(
        &self,
        mission_id: Uuid,
        bundle: &MissionBundle,
    ) -> Result<(), MissionError> {
        let workspace = self.root.ancestors().nth(3).unwrap_or(&self.root);
        let generated = Path::new(".workflow-tools")
            .join("mission")
            .join("missions")
            .join(mission_id.to_string())
            .join("generated");
        let consumers = [generated.join("README.md"), generated.join("ROADMAP.md")];
        let references = bundle
            .manifest
            .artifacts
            .iter()
            .chain(
                bundle
                    .waypoints
                    .iter()
                    .flat_map(|waypoint| &waypoint.artifacts),
            )
            .chain(
                bundle
                    .manifest
                    .operator_results
                    .iter()
                    .map(|result| &result.evidence_pointer),
            );
        let mut diagnostics = Vec::new();
        for reference in references {
            for consumer in &consumers {
                if let Err(diagnostic) = resolve_artifact_reference(workspace, consumer, reference)
                {
                    diagnostics.push(diagnostic);
                }
            }
        }
        if diagnostics.is_empty() {
            Ok(())
        } else {
            Err(MissionError::Validation(diagnostics.len(), diagnostics))
        }
    }
}
