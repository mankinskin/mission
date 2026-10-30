use super::*;
use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};

mod provenance;
mod references;
mod storage;

pub use provenance::{MissionArtifactProvenance, MissionFileFormat, MissionFileProvenance};
pub use references::resolve_artifact_reference;
pub(super) use references::validate_canonical_uri;
pub(super) use storage::validate_file_outputs;

pub const ARTIFACT_PROVENANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MissionFileRole {
    Authored,
    Generated,
    Helper,
}

fn artifact_diag(code: &'static str, pointer: &str, message: &str) -> Diagnostic {
    diag(
        code,
        Path::new("artifact-provenance.json"),
        pointer,
        None,
        message,
    )
}

fn reference_diag(code: &'static str, source: &Path, pointer: &str, message: &str) -> Diagnostic {
    diag(code, source, pointer, None, message)
}

fn validation_error(code: &'static str, path: &Path, message: &str) -> MissionError {
    MissionError::Validation(
        1,
        vec![artifact_diag(code, &path.display().to_string(), message)],
    )
}
