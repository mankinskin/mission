use mission::{
    CommitCheckpoint, DirectWaypoint, MISSION_WORKSPACE_ENV, Operation, OperatorResult,
    RepairRequest, ValidationGate, WaypointState, execute_cli, resolve_workspace,
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

#[path = "mission/commands.rs"]
mod commands;
use commands::CommandOperation;

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
            CommandOperation::ActivateWaypoint {
                mission_id,
                waypoint_id,
                request_id,
                expected_revision,
                author,
            } => {
                let envelope = mission::named::call(
                    "mission_waypoint_activate",
                    serde_json::json!({
                        "workspace_root": workspace,
                        "mission_id": mission_id,
                        "waypoint_id": waypoint_id,
                        "request_id": request_id,
                        "expected_revision": expected_revision,
                        "author": author,
                    }),
                );
                succeeded = envelope.code == "ok";
                return Output::json(envelope);
            }
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
            CommandOperation::Intent { mission_id, action } => Operation::Intent {
                mission_id,
                operation: serde_json::from_str(&action)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
            },
            CommandOperation::RecordReview {
                mission_id,
                request_id,
                input,
            } => Operation::RecordReview {
                mission_id,
                request_id,
                input: serde_json::from_str(&input)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
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
            CommandOperation::GetOutputContract { mission_id } => {
                Operation::GetOutputContract { mission_id }
            }
            CommandOperation::GetApprovalEvent {
                mission_id,
                revision,
            } => Operation::GetApprovalEvent {
                mission_id,
                revision,
            },
            CommandOperation::RecordApprovalEvent { mission_id, json } => {
                Operation::RecordApprovalEvent { mission_id, json }
            }
            CommandOperation::PublishWithInputs {
                mission_id,
                manifest,
                expected_current_revision,
                inputs,
            } => Operation::PublishWithInputs {
                mission_id,
                manifest_path: manifest,
                expected_current_revision,
                inputs: serde_json::from_str(&inputs)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
            },
            CommandOperation::MigrateDossiers { dossier, dry_run } => Operation::MigrateDossiers {
                dossier_path: dossier,
                dry_run,
            },
            CommandOperation::MigrateTypedStores { roots } => {
                Operation::MigrateTypedStores { roots }
            }
            CommandOperation::RepairPreview { records } => Operation::RepairPreview {
                request: serde_json::from_str::<RepairRequest>(&records)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
            },
            CommandOperation::RepairApply {
                records,
                proposal_id,
            } => Operation::RepairApply {
                request: serde_json::from_str::<RepairRequest>(&records)
                    .map_err(|error| HarnessError::Arguments(error.to_string()))?,
                proposal_id,
            },
            CommandOperation::RepairJournal { journal_id } => {
                Operation::RepairJournal { journal_id }
            }
            CommandOperation::RepairResume { journal_id } => Operation::RepairResume { journal_id },
            CommandOperation::RepairRollback { journal_id } => {
                Operation::RepairRollback { journal_id }
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
