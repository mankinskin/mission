//! Versioned named MCP adapters over the existing Mission providers.

mod dispatch;
mod inputs;
pub use inputs::*;

use crate::{MissionError, MissionLifecycle};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    marker::PhantomData,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const CAPABILITY: &str = "mission.native-planning";
pub const VERSION: u32 = 1;

/// Retain typed discovery schemas while reporting deserialization failures
/// through the same tool-error envelope as provider failures.
pub struct Input<T>(pub Value, PhantomData<T>);
impl<'de, T> Deserialize<'de> for Input<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self(Value::deserialize(d)?, PhantomData))
    }
}
impl<T: JsonSchema> JsonSchema for Input<T> {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        T::schema_name()
    }
    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}

#[derive(Debug, Serialize)]
pub struct Envelope {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mission_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    pub capability: &'static str,
    pub version: u32,
    pub tool: String,
    pub workspace: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug)]
struct Error {
    code: String,
    message: String,
    details: Value,
}
impl Error {
    fn new(code: &str, message: impl ToString) -> Self {
        let mapped = match code {
            "stale_control_sequence" | "stale_plan_revision" | "stale_completion" => {
                "revision_conflict"
            }
            "operation_conflict" | "request_id_conflict" => "idempotency_conflict",
            "waypoint_absent" => "waypoint_not_found",
            "session_inactive" => "session_finished",
            "mission_unavailable" => "workspace_unavailable",
            "reason_required" => "invalid_request",
            other => other,
        };
        Self {
            code: mapped.into(),
            message: message.to_string(),
            details: json!({"provider_code": code}),
        }
    }
}
impl From<crate::intake::IntakeError> for Error {
    fn from(e: crate::intake::IntakeError) -> Self {
        let mut out = Self::new(e.code(), &e);
        if let crate::intake::IntakeError::Pending(p) = e {
            out.details = json!({"provider_code":"pending", "mission_id":p.mission_id,
                "persisted":p.persisted.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>()});
        }
        out
    }
}
impl From<crate::approval::ApprovalError> for Error {
    fn from(e: crate::approval::ApprovalError) -> Self {
        Self::new(e.code(), e.message())
    }
}
impl From<crate::LifecycleError> for Error {
    fn from(e: crate::LifecycleError) -> Self {
        Self::new(e.code(), &e)
    }
}
impl From<MissionError> for Error {
    fn from(e: MissionError) -> Self {
        let code = match &e {
            MissionError::StaleRevision { .. } => "revision_conflict",
            MissionError::NotFound(_) | MissionError::Io(_) => "workspace_unavailable",
            MissionError::Validation(_, ds) => match ds.first().map(|d| d.code) {
                Some("request-id-conflict" | "request_id_conflict") => "idempotency_conflict",
                Some("waypoint-not-found" | "waypoint-absent") => "waypoint_not_found",
                Some(code) => code,
                None => "invalid_request",
            },
            _ => "workspace_unavailable",
        };
        let mut out = Self::new(code, &e);
        if let MissionError::Validation(_, diagnostics) = &e {
            out.details["diagnostics"] = json!(diagnostics);
        }
        out
    }
}
impl From<session_api::SessionError> for Error {
    fn from(e: session_api::SessionError) -> Self {
        let provider = e.requester_code().unwrap_or("session_error");
        let code = match provider {
            "requester_decision_conflict" | "requester_challenge_conflict" => "approval_conflict",
            "requester_decision_pending" => "approval_pending",
            _ => "approval_authority_unavailable",
        };
        let mut error = Self::new(code, &e);
        error.details["provider_code"] = json!(provider);
        error
    }
}

fn parse<T: DeserializeOwned>(value: Value) -> Result<T, Error> {
    serde_json::from_value(value).map_err(|e| Error::new("invalid_request", e))
}
fn id(value: &str) -> Result<Uuid, Error> {
    Uuid::parse_str(value)
        .ok()
        .filter(|id| !id.is_nil())
        .ok_or_else(|| Error::new("invalid_request", "identity must be a non-nil UUID"))
}
fn root(value: &Path) -> Result<PathBuf, Error> {
    if !value.is_absolute() || !value.is_dir() {
        return Err(Error::new(
            "workspace_unavailable",
            "owning root must be an existing absolute directory",
        ));
    }
    crate::resolve_workspace(Some(value), None, value)
        .map(|(root, _)| root)
        .map_err(|e| Error::new("workspace_unavailable", e))
}
fn session(root: &Path, explicit: Option<&Path>) -> Result<session_api::SessionStoreConfig, Error> {
    let selected = explicit
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.join(".workflow-tools/session"));
    Ok(session_api::SessionStoreConfig::new(
        crate::resolve_workspace(Some(&selected), None, root)
            .map_err(|e| Error::new("workspace_unavailable", e))?
            .0,
    ))
}
fn nonempty(value: &str, field: &str) -> Result<(), Error> {
    if value.trim().is_empty() {
        Err(Error::new(
            "invalid_request",
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(())
    }
}
fn value<T: Serialize>(result: T) -> Result<Value, Error> {
    serde_json::to_value(result).map_err(|e| Error::new("serialization_error", e))
}
fn workspace(root: &Path, mission: Option<Uuid>) -> Value {
    let mut paths = json!({"workspace_root":root});
    if let Some(id) = mission {
        let directory = root
            .join(".workflow-tools/mission/missions")
            .join(id.to_string());
        paths["mission_dir"] = json!(directory);
        paths["source_path"] = json!(directory.join("source.json"));
        paths["generated_plan_entry"] = json!(directory.join("generated/README.md"));
    }
    paths
}

/// Run a named operation without consulting the server's launch directory.
pub fn call(tool: &str, arguments: Value) -> Envelope {
    let mission_id = arguments
        .get("mission_id")
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok());
    let root_path = arguments
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from);
    let mut out = Envelope {
        code: "ok".into(),
        message: "completed".into(),
        mission_id,
        revision: None,
        capability: CAPABILITY,
        version: VERSION,
        tool: tool.into(),
        workspace: root_path
            .as_deref()
            .map(|r| workspace(r, mission_id))
            .unwrap_or(Value::Null),
        result: None,
        details: None,
    };
    match dispatch::run(tool, arguments) {
        Ok((id, revision, paths, result)) => {
            out.mission_id = Some(id);
            out.revision = Some(revision);
            out.workspace = paths;
            out.result = Some(result);
        }
        Err(e) => {
            out.code = e.code;
            out.message = e.message;
            out.details = Some(e.details);
            if let (Some(root), Some(id)) = (root_path, mission_id) {
                out.revision = MissionLifecycle::open(&root)
                    .review_plan(id)
                    .ok()
                    .map(|p| p.revision);
            }
        }
    }
    out
}
