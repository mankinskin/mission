use std::path::PathBuf;

use mission::{
    CommitCheckpoint, DirectWaypoint, Operation, OperatorResult, ValidationGate, WaypointState,
    execute_mcp,
};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use transport_harness::HarnessError;
use uuid::Uuid;

#[derive(Clone)]
struct MissionServer {
    workspace: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MissionOperationInput {
    operation: String,
    #[serde(default)]
    mission_id: Option<String>,
    #[serde(default)]
    manifest_path: Option<PathBuf>,
    #[serde(default)]
    expected_current_revision: Option<u64>,
    #[serde(default)]
    dossier_path: Option<PathBuf>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    objective: Option<String>,
    #[serde(default)]
    waypoints: Option<Vec<McpWaypoint>>,
    #[serde(default)]
    waypoint_id: Option<String>,
    #[serde(default)]
    waypoint_title: Option<String>,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    validation: Option<Vec<McpValidationGate>>,
    #[serde(default)]
    commit_checkpoint: Option<McpCheckpoint>,
    #[serde(default)]
    non_goals: Vec<String>,
    #[serde(default)]
    execution_order: Vec<String>,
    #[serde(default)]
    waypoint_dependencies: std::collections::BTreeMap<String, Vec<String>>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    outcome: Option<String>,
    #[serde(default)]
    operator_command: Option<String>,
    #[serde(default)]
    evidence_pointer: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpWaypoint {
    waypoint_id: String,
    title: String,
    #[serde(default)]
    depends_on: Vec<String>,
    prompt: String,
    validation: Vec<McpValidationGate>,
    commit_checkpoint: McpCheckpoint,
    #[serde(default)]
    non_goals: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpValidationGate {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpCheckpoint {
    message: String,
}

impl From<McpWaypoint> for DirectWaypoint {
    fn from(input: McpWaypoint) -> Self {
        Self {
            waypoint_id: input.waypoint_id,
            title: input.title,
            depends_on: input.depends_on,
            prompt: input.prompt,
            validation: input
                .validation
                .into_iter()
                .map(|gate| ValidationGate {
                    command: gate.command,
                    cwd: gate.cwd,
                })
                .collect(),
            commit_checkpoint: CommitCheckpoint {
                message: input.commit_checkpoint.message,
            },
            non_goals: input.non_goals,
        }
    }
}

impl MissionServer {
    fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl MissionServer {
    #[tool(
        name = "mission_operation",
        description = "Run create_mission, edit_mission_objective, add_waypoint, edit_waypoint, order_waypoints, record_progress, record_validation_result, validate_mission, or publish_mission directly by stored ID. Waypoints require prompt, validation commands and commit_checkpoint; results record operator evidence without executing commands. Existing get, validate-preview, import, render-preview, publish, check-generated and migrate-dossiers remain supported. Return a structured Mission status snapshot."
    )]
    async fn operation(
        &self,
        Parameters(input): Parameters<MissionOperationInput>,
    ) -> Result<CallToolResult, McpError> {
        let required_id = || {
            input
                .mission_id
                .as_deref()
                .ok_or_else(|| McpError::invalid_params("mission_id is required", None))
                .and_then(|value| {
                    Uuid::parse_str(value)
                        .map_err(|_| McpError::invalid_params("mission_id must be a UUID", None))
                })
        };
        let required_manifest = || {
            input
                .manifest_path
                .clone()
                .ok_or_else(|| McpError::invalid_params("manifest_path is required", None))
        };
        let required = |value: &Option<String>, name| {
            value
                .clone()
                .ok_or_else(|| McpError::invalid_params(format!("{name} is required"), None))
        };
        let waypoint = || {
            Ok(DirectWaypoint::from(McpWaypoint {
                waypoint_id: required(&input.waypoint_id, "waypoint_id")?,
                title: required(&input.waypoint_title, "waypoint_title")?,
                depends_on: input.depends_on.clone(),
                prompt: required(&input.prompt, "prompt")?,
                validation: input
                    .validation
                    .clone()
                    .ok_or_else(|| McpError::invalid_params("validation is required", None))?,
                commit_checkpoint: input.commit_checkpoint.clone().ok_or_else(|| {
                    McpError::invalid_params("commit_checkpoint is required", None)
                })?,
                non_goals: input.non_goals.clone(),
            }))
        };
        let operation = match input.operation.as_str() {
            "create_mission" => Operation::CreateMission {
                title: required(&input.title, "title")?,
                objective: required(&input.objective, "objective")?,
                waypoints: input
                    .waypoints
                    .clone()
                    .ok_or_else(|| McpError::invalid_params("waypoints is required", None))?
                    .into_iter()
                    .map(DirectWaypoint::from)
                    .collect(),
            },
            "edit_mission_objective" => Operation::EditMissionObjective {
                mission_id: required_id()?,
                objective: required(&input.objective, "objective")?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "add_waypoint" => Operation::AddWaypoint {
                mission_id: required_id()?,
                waypoint: waypoint()?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "edit_waypoint" => Operation::EditWaypoint {
                mission_id: required_id()?,
                waypoint: waypoint()?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "order_waypoints" => Operation::OrderWaypoints {
                mission_id: required_id()?,
                execution_order: input.execution_order.clone(),
                waypoint_dependencies: input.waypoint_dependencies.clone(),
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "record_progress" => Operation::RecordProgress {
                mission_id: required_id()?,
                waypoint_id: required(&input.waypoint_id, "waypoint_id")?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
                state: match required(&input.state, "state")?.as_str() {
                    "pending" => WaypointState::Pending,
                    "in-progress" => WaypointState::InProgress,
                    "completed" => WaypointState::Completed,
                    "blocked" => WaypointState::Blocked,
                    _ => return Err(McpError::invalid_params("invalid state", None)),
                },
            },
            "record_validation_result" => Operation::RecordValidationResult {
                mission_id: required_id()?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
                result: OperatorResult {
                    waypoint_id: required(&input.waypoint_id, "waypoint_id")?,
                    outcome: required(&input.outcome, "outcome")?,
                    operator_command: required(&input.operator_command, "operator_command")?,
                    evidence_pointer: required(&input.evidence_pointer, "evidence_pointer")?,
                },
            },
            "validate_mission" => Operation::ValidateMission {
                mission_id: required_id()?,
            },
            "publish_mission" => Operation::PublishMission {
                mission_id: required_id()?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "get" | "get_mission" => Operation::Get {
                mission_id: required_id()?,
            },
            "validate-preview" => Operation::ValidatePreview {
                manifest_path: required_manifest()?,
            },
            "import" => Operation::Import {
                mission_id: required_id()?,
                manifest_path: required_manifest()?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "render-preview" => Operation::RenderPreview {
                mission_id: required_id()?,
            },
            "publish" => Operation::Publish {
                mission_id: required_id()?,
                manifest_path: required_manifest()?,
                expected_current_revision: input.expected_current_revision.ok_or_else(|| {
                    McpError::invalid_params("expected_current_revision is required", None)
                })?,
            },
            "check-generated" => Operation::CheckGenerated {
                mission_id: required_id()?,
            },
            "migrate-dossiers" => Operation::MigrateDossiers {
                dossier_path: input.dossier_path.clone(),
                dry_run: input.dry_run,
            },
            _ => return Err(McpError::invalid_params("unknown operation", None)),
        };
        let snapshot = execute_mcp(&self.workspace, operation);
        let text = serde_json::to_string(&snapshot)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[tool_handler]
impl ServerHandler for MissionServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            instructions: Some(
                "Mission roadmap operations use the canonical workspace store.".into(),
            ),
            ..Default::default()
        }
    }
}

fn main() -> Result<(), HarnessError> {
    transport_harness::mcp::run(MissionServer::new(
        std::env::current_dir().map_err(HarnessError::Io)?,
    ))
}
