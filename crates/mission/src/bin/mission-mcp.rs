use std::path::PathBuf;

use mission::{Operation, execute_mcp};
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
        description = "Run mission get, validate-preview, import, render-preview, publish, check-generated, or migrate-dossiers and return a structured Mission status snapshot."
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
        let operation = match input.operation.as_str() {
            "get" => Operation::Get {
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
