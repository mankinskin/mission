use mission::{Operation, execute_cli};
use std::process::ExitCode;
use transport_harness::{
    HarnessError, Output,
    cli::clap::{Parser, Subcommand},
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "mission")]
struct Command {
    #[arg(long, default_value = ".")]
    workspace: std::path::PathBuf,
    #[command(subcommand)]
    operation: CommandOperation,
}

#[derive(Subcommand)]
enum CommandOperation {
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
}

fn main() -> Result<ExitCode, HarnessError> {
    let mut succeeded = false;
    transport_harness::cli::run(|command: Command| {
        let operation = match command.operation {
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
        };
        let snapshot = execute_cli(&command.workspace, operation);
        succeeded = snapshot.status == "ok";
        Output::json(snapshot)
    })?;
    Ok(if succeeded {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    })
}
