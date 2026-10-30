use std::{
    fs,
    path::{Path, PathBuf},
};

use mission::{
    BundleManifest, CommitCheckpoint, MissionBundle, MissionStore, Operation, Scope,
    ValidationGate, WaypointFragment, WaypointMode, WaypointState, execute_cli,
};
use tempfile::TempDir;
use ticket_api::storage::TicketStore;
use uuid::Uuid;

fn roadmap(title: &str, waypoints: &[(&str, &str, Option<usize>)]) -> String {
    let mut contents = format!(
        "# Roadmap: {title}\n\n## Outcome Summary\n\nPreserve this legacy roadmap.\n\n## Roadmap Waypoints\n\n"
    );
    for (index, (waypoint_title, part, dependency)) in waypoints.iter().enumerate() {
        contents.push_str(&format!(
            "### W{}. {waypoint_title}\n\n| | |\n|---|---|\n| **Status** | completed |\n| **Scope** | single-session |\n| **Mode** | Execution side effect |\n| **Session package** | {} |\n| **Part** | [{part}]({part}) |\n| **Prompt** | Implement {waypoint_title}. |\n| **Artifacts** | ce://fixture/spec/source-artifact |\n| **Non-goal** | unrelated work |\n| **Validate** | `cargo test -p mission` |\n| **Commit checkpoint** | `feat: {waypoint_title}` |\n",
            index + 1,
            waypoint_title.to_ascii_lowercase().replace(' ', "-")
        ));
        if let Some(dependency) = dependency {
            contents.push_str(&format!("| **Depends** | W{} |\n", dependency + 1));
        }
        contents.push('\n');
    }
    contents
}

fn authored_roadmap() -> String {
    "# Roadmap: Authored Mission

## Outcome

Preserve the authoring contract without adding execution metadata.

## Roadmap Waypoints

### W1. Inspect the current state

| | |
|---|---|
| **Status** | done |
| **Scope** | single-session |
| **Mode** | Dependent read-only waypoint |
| **Expected state** | The current state is recorded. |
| **Session package** | inspect-current-state |
| **Part** | [01-first.md](01-first.md) |
| **Prompt** | Inspect the current state. |
| **Artifacts** | ce://fixture/spec/source-artifact |
| **Non-goal** | unrelated work |
| **Validate** | `cargo test -p mission` |
| **Commit checkpoint** | none (read-only waypoint) |

### W2. Apply the change

| | |
|---|---|
| **Status** | pending |
| **Scope** | single-session |
| **Mode** | Execution side effect |
| **Expected state** | The change is applied. |
| **Session package** | apply-change |
| **Part** | [02-second.md](02-second.md) |
| **Prompt** | Apply the change. |
| **Artifacts** | ce://fixture/spec/source-artifact |
| **Non-goal** | unrelated work |
| **Validate** | `cargo test -p mission` |
| **Commit checkpoint** | feat: apply change |
| **Depends on** | W1 |
"
    .into()
}

fn write_dossier(root: &Path, name: &str, contents: &str) -> PathBuf {
    let dossier = root.join("transcripts").join(name);
    fs::create_dir_all(&dossier).unwrap();
    fs::write(dossier.join("ROADMAP.md"), contents).unwrap();
    fs::write(
        dossier.join("01-first.md"),
        "Previous Part: [Old](old.md) | Next Part: [Old](next.md)\n\n## Outcome\n\nPreserve part content.\n",
    )
    .unwrap();
    fs::write(
        dossier.join("02-second.md"),
        "## Outcome\n\nPreserve the second Part.\n",
    )
    .unwrap();
    dossier
}

fn completed_child_bundle() -> MissionBundle {
    MissionBundle {
        manifest: BundleManifest {
            schema_version: 1,
            revision: 1,
            title: "Accepted child".into(),
            objective: "Complete child work.".into(),
            waypoint_files: vec![],
            execution_order: vec!["child-work".into()],
            artifacts: vec![],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
            operator_results: vec![],
        },
        waypoints: vec![WaypointFragment {
            outcome_plan: None,
            waypoint_id: "child-work".into(),
            title: "Child work".into(),
            depends_on: vec![],
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Completed),
            ticket_ref: None,
            relationships: Default::default(),
            child_mission_ref: None,
            session_package: "child-work".into(),
            prompt: "Complete the child work.".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "cargo test -p mission".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "test: complete child mission".into(),
            },
            part_markdown: "## Outcome\n\nChild work is complete.\n".into(),
        }],
    }
}

#[path = "dossier_migration/idempotence_and_sources.rs"]
mod idempotence_and_sources;
#[path = "dossier_migration/legacy_parsing.rs"]
mod legacy_parsing;
#[path = "dossier_migration/ownership_and_state.rs"]
mod ownership_and_state;
