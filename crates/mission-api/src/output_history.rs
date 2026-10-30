use super::*;
use sha2::{Digest, Sha256};
use std::{io::Write, time::SystemTime};

/// Additional request material; continuations are independent, create-new inputs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MissionOutputInputs {
    pub merged_request: Option<String>,
    #[serde(default)]
    pub continuations: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputFile {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub role: String,
    pub format: String,
}

/// Pre-reaction inventory. Its digest is a binding identity, not authentication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionOutputContract {
    pub schema_version: u32,
    pub mission_id: Uuid,
    pub revision: u64,
    pub revision_label: String,
    pub renderer: String,
    pub producer: String,
    pub purpose: String,
    pub files: Vec<OutputFile>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalDecision {
    Approve,
    ChangesRequested,
}

/// Caller-supplied reaction. Time and event identity are assigned on recording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ApprovalEventRequest {
    pub revision: u64,
    pub decision: ApprovalDecision,
    /// `sha256:<digest>` of the exact contract JSON bytes, not a signature.
    pub signature: String,
    pub actor: String,
    pub actor_source: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionApprovalEvent {
    pub schema_version: u32,
    pub event_id: Uuid,
    pub mission_id: Uuid,
    pub recorded_at_unix_ms: u64,
    #[serde(flatten)]
    pub reaction: ApprovalEventRequest,
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn timestamp() -> Result<u64, MissionError> {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .ok_or_else(|| invalid("System clock cannot supply a recording timestamp."))
}

impl MissionStore {
    fn event_metadata(&self) -> Result<(Uuid, u64), MissionError> {
        #[cfg(test)]
        if let Some(metadata) = self.test_event_metadata {
            return Ok(metadata);
        }
        Ok((Uuid::new_v4(), timestamp()?))
    }
}

mod history;
mod projection;
mod store;

pub(super) use history::*;
pub(super) use projection::*;
