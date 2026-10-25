use mission::{
    CommitCheckpoint, DirectWaypoint, MISSION_WORKSPACE_ENV, Operation, OperatorResult,
    ValidationGate, WaypointState, execute_cli, resolve_workspace,
};
use std::process::ExitCode;
use transport_harness::{
    HarnessError, Output,
    cli::clap::{Args, Parser, Subcommand, error::ErrorKind},
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "mission")]
struct Command {
    #[arg(long)]
    workspace: Option<std::path::PathBuf>,
    #[command(subcommand)]
    operation: CommandOperation,
}

#[derive(Subcommand)]
enum CommandOperation {
    CreateMission {
        title: String,
        objective: String,
        waypoints: String,
    },
    CreateDraft {
        title: String,
        objective: String,
    },
    SaveDraft {
        mission_id: Uuid,
        draft: String,
        expected_current_revision: u64,
    },
    PublishDraft {
        mission_id: Uuid,
        draft: String,
        expected_current_revision: u64,
    },
    GetMissionSource {
        mission_id: Uuid,
    },
    RenderMission {
        mission_id: Uuid,
    },
    EditMissionObjective {
        mission_id: Uuid,
        objective: String,
        expected_current_revision: u64,
    },
    AddWaypoint {
        mission_id: Uuid,
        waypoint_id: String,
        title: String,
        #[command(flatten)]
        content: WaypointContent,
        expected_current_revision: u64,
    },
    EditWaypoint {
        mission_id: Uuid,
        waypoint_id: String,
        title: String,
        #[command(flatten)]
        content: WaypointContent,
        expected_current_revision: u64,
    },
    OrderWaypoints {
        mission_id: Uuid,
        #[arg(value_delimiter = ',', required = true, num_args = 1)]
        waypoint_ids: Vec<String>,
        expected_current_revision: u64,
        #[arg(long)]
        waypoint_dependencies: Option<String>,
    },
    RecordProgress {
        mission_id: Uuid,
        waypoint_id: String,
        state: String,
        expected_current_revision: u64,
    },
    RecordValidationResult {
        mission_id: Uuid,
        waypoint_id: String,
        outcome: String,
        operator_command: String,
        evidence_pointer: String,
        expected_current_revision: u64,
    },
    ValidateMission {
        mission_id: Uuid,
    },
    PublishMission {
        mission_id: Uuid,
        expected_current_revision: u64,
    },
    #[command(alias = "get-mission")]
    Get {
        mission_id: Uuid,
    },
    ValidatePreview {
        manifest: std::path::PathBuf,
    },
    Import {
        mission_id: Uuid,
        manifest: std::path::PathBuf,
        expected_current_revision: u64,
    },
    RenderPreview {
        mission_id: Uuid,
    },
    Publish {
        mission_id: Uuid,
        manifest: std::path::PathBuf,
        expected_current_revision: u64,
    },
    CheckGenerated {
        mission_id: Uuid,
    },
    MigrateDossiers {
        #[arg(long)]
        dossier: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    MigrateTypedStores {
        #[arg(long = "root")]
        roots: Vec<std::path::PathBuf>,
    },
}

#[derive(Args)]
struct WaypointContent {
    #[arg(long, value_delimiter = ',')]
    depends_on: Vec<String>,
    #[arg(long)]
    prompt: String,
    #[arg(long = "validation-command", required = true)]
    validation_commands: Vec<String>,
    #[arg(long)]
    validation_cwd: Option<String>,
    #[arg(long)]
    checkpoint: String,
    #[arg(long = "non-goal")]
    non_goals: Vec<String>,
}

impl WaypointContent {
    fn waypoint(self, waypoint_id: String, title: String) -> DirectWaypoint {
        DirectWaypoint {
            waypoint_id,
            title,
            depends_on: self.depends_on,
            prompt: self.prompt,
            validation: self
                .validation_commands
                .into_iter()
                .map(|command| ValidationGate {
                    command,
                    cwd: self.validation_cwd.clone(),
                })
                .collect(),
            commit_checkpoint: CommitCheckpoint {
                message: self.checkpoint,
            },
            non_goals: self.non_goals,
        }
    }
}

fn main() -> Result<ExitCode, HarnessError> {
    let launch = match Command::try_parse_from(std::env::args_os()) {
        Ok(command) => command,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return Ok(ExitCode::SUCCESS);
        }
        Err(error) => return Err(HarnessError::Arguments(error.to_string())),
    };
    let cwd = std::env::current_dir().map_err(HarnessError::Io)?;
    let configured = std::env::var_os(MISSION_WORKSPACE_ENV);
    let (workspace, _) =
        resolve_workspace(launch.workspace.as_deref(), configured.as_deref(), &cwd)
            .map_err(HarnessError::Arguments)?;

    let mut succeeded = false;
    transport_harness::cli::run(|command: Command| {
        let operation = match command.operation {
            CommandOperation::CreateMission {
                title,
                objective,
                waypoints,
            } => Operation::CreateMission {
                title,
                objective,
                waypoints: serde_json::from_str(&waypoints)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
            },
            CommandOperation::CreateDraft { title, objective } => {
                Operation::CreateDraft { title, objective }
            }
            CommandOperation::SaveDraft {
                mission_id,
                draft,
                expected_current_revision,
            } => Operation::SaveDraft {
                mission_id,
                draft: serde_json::from_str(&draft)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
                expected_current_revision,
            },
            CommandOperation::PublishDraft {
                mission_id,
                draft,
                expected_current_revision,
            } => Operation::PublishDraft {
                mission_id,
                draft: serde_json::from_str(&draft)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
                expected_current_revision,
            },
            CommandOperation::GetMissionSource { mission_id } => {
                Operation::GetMissionSource { mission_id }
            }
            CommandOperation::RenderMission { mission_id } => {
                Operation::RenderMission { mission_id }
            }
            CommandOperation::EditMissionObjective {
                mission_id,
                objective,
                expected_current_revision,
            } => Operation::EditMissionObjective {
                mission_id,
                objective,
                expected_current_revision,
            },
            CommandOperation::AddWaypoint {
                mission_id,
                waypoint_id,
                title,
                content,
                expected_current_revision,
            } => Operation::AddWaypoint {
                mission_id,
                waypoint: content.waypoint(waypoint_id, title),
                expected_current_revision,
            },
            CommandOperation::EditWaypoint {
                mission_id,
                waypoint_id,
                title,
                content,
                expected_current_revision,
            } => Operation::EditWaypoint {
                mission_id,
                waypoint: content.waypoint(waypoint_id, title),
                expected_current_revision,
            },
            CommandOperation::OrderWaypoints {
                mission_id,
                waypoint_ids,
                expected_current_revision,
                waypoint_dependencies,
            } => Operation::OrderWaypoints {
                mission_id,
                execution_order: waypoint_ids,
                waypoint_dependencies: waypoint_dependencies
                    .map(|value| serde_json::from_str(&value))
                    .transpose()
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?
                    .unwrap_or_default(),
                expected_current_revision,
            },
            CommandOperation::RecordProgress {
                mission_id,
                waypoint_id,
                state,
                expected_current_revision,
            } => Operation::RecordProgress {
                mission_id,
                waypoint_id,
                expected_current_revision,
                state: match state.as_str() {
                    "pending" => WaypointState::Pending,
                    "in-progress" => WaypointState::InProgress,
                    "completed" => WaypointState::Completed,
                    "blocked" => WaypointState::Blocked,
                    _ => {
                        return Err(HarnessError::Arguments(
                            "state must be pending, in-progress, completed, or blocked".into(),
                        ));
                    }
                },
            },
            CommandOperation::RecordValidationResult {
                mission_id,
                waypoint_id,
                outcome,
                operator_command,
                evidence_pointer,
                expected_current_revision,
            } => Operation::RecordValidationResult {
                mission_id,
                result: OperatorResult {
                    waypoint_id,
                    outcome,
                    operator_command,
                    evidence_pointer,
                },
                expected_current_revision,
            },
            CommandOperation::ValidateMission { mission_id } => {
                Operation::ValidateMission { mission_id }
            }
            CommandOperation::PublishMission {
                mission_id,
                expected_current_revision,
            } => Operation::PublishMission {
                mission_id,
                expected_current_revision,
            },
            CommandOperation::Get { mission_id } => Operation::Get { mission_id },
            CommandOperation::ValidatePreview { manifest } => Operation::ValidatePreview {
                manifest_path: manifest,
            },
            CommandOperation::Import {
                mission_id,
                manifest,
                expected_current_revision,
            } => Operation::Import {
                mission_id,
                manifest_path: manifest,
                expected_current_revision,
            },
            CommandOperation::RenderPreview { mission_id } => {
                Operation::RenderPreview { mission_id }
            }
            CommandOperation::Publish {
                mission_id,
                manifest,
                expected_current_revision,
            } => Operation::Publish {
                mission_id,
                manifest_path: manifest,
                expected_current_revision,
            },
            CommandOperation::CheckGenerated { mission_id } => {
                Operation::CheckGenerated { mission_id }
            }
            CommandOperation::MigrateDossiers { dossier, dry_run } => Operation::MigrateDossiers {
                dossier_path: dossier,
                dry_run,
            },
            CommandOperation::MigrateTypedStores { roots } => {
                Operation::MigrateTypedStores { roots }
            }
        };
        let snapshot = execute_cli(&workspace, operation);
        succeeded = snapshot.status == "ok";
        Output::json(snapshot)
    })?;
    Ok(if succeeded {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
