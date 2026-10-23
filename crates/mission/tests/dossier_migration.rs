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
            "### W{}. {waypoint_title}\n\n| | |\n|---|---|\n| **Status** | completed |\n| **Scope** | single-session |\n| **Mode** | Execution side effect |\n| **Session package** | {} |\n| **Part** | [{part}]({part}) |\n| **Prompt** | Implement {waypoint_title}. |\n| **Artifacts** | source artifact |\n| **Non-goal** | unrelated work |\n| **Validate** | `cargo test -p mission` |\n| **Commit checkpoint** | `feat: {waypoint_title}` |\n",
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
| **Artifacts** | source artifact |
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
| **Artifacts** | source artifact |
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
        },
        waypoints: vec![WaypointFragment {
            waypoint_id: "child-work".into(),
            title: "Child work".into(),
            depends_on: vec![],
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Completed),
            ticket_ref: None,
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

#[test]
fn migration_is_idempotent() {
    let workspace = TempDir::new().unwrap();
    let dossier = write_dossier(
        workspace.path(),
        "fixture",
        &roadmap("Fixture Mission", &[("Alpha", "01-first.md", None)]),
    );
    let source_before = fs::read(dossier.join("ROADMAP.md")).unwrap();
    let part_before = fs::read(dossier.join("01-first.md")).unwrap();

    let dry_run = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: None,
            dry_run: true,
        },
    );
    assert_eq!(dry_run.status, "ok");
    let report = dry_run.migration_report.unwrap();
    assert_eq!(report.would_migrate, 1, "{:?}", report.records);
    assert!(!workspace.path().join(".workflow-tools/mission").exists());

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: None,
            dry_run: false,
        },
    );
    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let mission_id = report.records[0].mission_id.unwrap();
    let accepted = MissionStore::open(workspace.path())
        .get(mission_id)
        .unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 1);
    assert!(
        accepted
            .bundle
            .manifest
            .notes
            .contains(&"legacy-source: transcripts/fixture".to_string())
    );
    assert!(accepted.bundle.waypoints[0].waypoint_id == "alpha");
    assert!(
        workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .join(mission_id.to_string())
            .join("generated/ROADMAP.md")
            .is_file()
    );

    let repeated = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: None,
            dry_run: false,
        },
    );
    assert_eq!(repeated.status, "ok");
    let report = repeated.migration_report.unwrap();
    assert_eq!(report.migrated, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(
        MissionStore::open(workspace.path())
            .get(mission_id)
            .unwrap()
            .bundle
            .manifest
            .revision,
        1
    );

    let readback = execute_cli(workspace.path(), Operation::Get { mission_id });
    assert_eq!(readback.status, "ok");
    assert_eq!(readback.accepted.unwrap(), accepted);
    assert_eq!(fs::read(dossier.join("ROADMAP.md")).unwrap(), source_before);
    assert_eq!(fs::read(dossier.join("01-first.md")).unwrap(), part_before);
}

#[test]
fn migration_imports_current_roadmap_only_and_preserves_history_sources() {
    let workspace = TempDir::new().unwrap();
    let dossier = write_dossier(
        workspace.path(),
        "current-only",
        &roadmap(
            "Historical Mission",
            &[
                ("Alpha", "01-first.md", None),
                ("Beta", "02-second.md", Some(0)),
            ],
        ),
    );
    let historical = roadmap(
        "Historical Mission",
        &[
            ("Old Alpha", "missing/old-alpha.md", None),
            ("Old Beta", "missing/old-beta.md", Some(0)),
        ],
    );
    fs::write(dossier.join("ROADMAP.v1.md"), historical).unwrap();
    let current = roadmap(
        "Historical Mission",
        &[
            ("Beta", "02-second.md", None),
            ("Alpha", "01-first.md", Some(0)),
        ],
    );
    fs::write(dossier.join("ROADMAP.md"), &current).unwrap();
    let source_before = fs::read(dossier.join("ROADMAP.md")).unwrap();
    let historical_before = fs::read(dossier.join("ROADMAP.v1.md")).unwrap();

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: None,
            dry_run: false,
        },
    );
    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let mission_id = report.records[0].mission_id.unwrap();

    let store = MissionStore::open(workspace.path());
    let accepted = store.get(mission_id).unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 1);
    assert_eq!(
        store.revision_mapping(mission_id, 1).unwrap().labels["W1"],
        "beta"
    );
    assert!(store.revision_mapping(mission_id, 2).is_err());
    assert_eq!(accepted.bundle.manifest.execution_order, ["beta", "alpha"]);
    assert!(
        accepted
            .bundle
            .manifest
            .notes
            .contains(&"legacy-history-excluded: transcripts/current-only/ROADMAP.v1.md".into())
    );
    assert_eq!(fs::read(dossier.join("ROADMAP.md")).unwrap(), source_before);
    assert_eq!(
        fs::read(dossier.join("ROADMAP.v1.md")).unwrap(),
        historical_before
    );
    assert!(dossier.join("01-first.md").is_file());
    assert!(dossier.join("02-second.md").is_file());
}

#[test]
fn ambiguous_source_is_reported_blocked() {
    let workspace = TempDir::new().unwrap();
    write_dossier(
        workspace.path(),
        "ambiguous",
        &roadmap(
            "Ambiguous Mission",
            &[
                ("Duplicate", "01-first.md", None),
                ("Duplicate", "02-second.md", Some(0)),
            ],
        ),
    );

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: None,
            dry_run: true,
        },
    );
    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.blocked, 1);
    assert_eq!(report.records[0].outcome, "blocked");
    assert!(
        report.records[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("duplicate waypoint ID"),
        "{:?}",
        report.records
    );
    assert!(!workspace.path().join(".workflow-tools/mission").exists());
}

#[test]
fn ticket_backed_source_status_remains_externally_owned() {
    let workspace = TempDir::new().unwrap();
    let ticket_id = Uuid::parse_str("1486c2f9-301e-4709-a8d0-cae3c4e6f5d4").unwrap();
    TicketStore::init(workspace.path())
        .unwrap()
        .create(
            Some(ticket_id),
            "tracker-improvement",
            Some("Ticket-backed migration fixture"),
            Some("planning"),
            Default::default(),
            None,
            Some("Provide the external state for this migration fixture."),
        )
        .unwrap();
    let contents = roadmap("Ticket Mission", &[("Implementation", "01-first.md", None)]).replace(
        "| **Scope** | single-session |",
        "| **Scope** | ticket-backed |\n| **Ticket** | ce://default/ticket/1486c2f9-301e-4709-a8d0-cae3c4e6f5d4 |",
    );
    write_dossier(workspace.path(), "ticket-backed", &contents);

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: None,
            dry_run: false,
        },
    );
    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let accepted = MissionStore::open(workspace.path())
        .get(report.records[0].mission_id.unwrap())
        .unwrap();
    assert!(accepted.bundle.waypoints[0].state.is_none());
    assert_eq!(
        accepted.bundle.waypoints[0].ticket_ref.as_deref(),
        Some("ce://default/ticket/1486c2f9-301e-4709-a8d0-cae3c4e6f5d4")
    );
}

#[test]
fn migration_renders_accepted_child_mission_state() {
    let workspace = TempDir::new().unwrap();
    let child_id = Uuid::new_v4();
    let store = MissionStore::open(workspace.path());
    store.import(child_id, completed_child_bundle(), 0).unwrap();
    let contents = roadmap("Parent Mission", &[("Child work", "01-first.md", None)]).replace(
        "| **Scope** | single-session |",
        &format!(
            "| **Scope** | single-session |\n| **Child mission** | ce://default/mission/{child_id} |"
        ),
    );
    let dossier = write_dossier(workspace.path(), "parent-with-child", &contents);

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let parent_id = report.records[0].mission_id.unwrap();
    let accepted = store.get(parent_id).unwrap();
    assert_eq!(
        accepted.bundle.waypoints[0].child_mission_ref.as_deref(),
        Some(format!("ce://default/mission/{child_id}").as_str())
    );
    assert!(accepted.bundle.waypoints[0].state.is_none());
    let generated = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(parent_id.to_string())
        .join("generated/ROADMAP.md");
    let rendered = fs::read_to_string(generated).unwrap();
    assert!(rendered.contains("| **State** | completed |"));
    assert!(rendered.contains("external-status-snapshot: ticket-status-v1:"));
}

#[test]
fn migration_accepts_case_variant_headings_and_preserves_authored_part_content() {
    let workspace = TempDir::new().unwrap();
    let contents = roadmap("Case Variant Mission", &[("Alpha", "01-first.md", None)])
        .replace("## Outcome Summary", "## Outcome summary")
        .replace(
            "## Roadmap Waypoints",
            "## Validation gates\n\n- `cargo test -p first`\n- `cargo test -p second`\n\n## Roadmap Waypoints",
        );
    let dossier = write_dossier(workspace.path(), "case-variant", &contents);
    fs::write(
        dossier.join("01-first.md"),
        "## Outcome\n\nPreserve part content.\n\n## Expected state\n\nAuthored state remains intact.\n",
    )
    .unwrap();

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );
    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let accepted = MissionStore::open(workspace.path())
        .get(report.records[0].mission_id.unwrap())
        .unwrap();
    assert_eq!(
        accepted.bundle.manifest.objective,
        "Preserve this legacy roadmap."
    );
    assert_eq!(
        accepted
            .bundle
            .manifest
            .validation_gates
            .iter()
            .map(|gate| gate.command.as_str())
            .collect::<Vec<_>>(),
        ["cargo test -p first", "cargo test -p second"]
    );
    assert!(
        accepted.bundle.waypoints[0]
            .part_markdown
            .contains("## Expected state\n\nAuthored state remains intact.")
    );
}

#[test]
fn migration_rejects_missing_or_empty_outcome_summary() {
    for (name, contents) in [
        (
            "missing-outcome",
            roadmap("Missing Outcome", &[("Alpha", "01-first.md", None)]).replace(
                "## Outcome Summary\n\nPreserve this legacy roadmap.\n\n",
                "",
            ),
        ),
        (
            "empty-outcome",
            roadmap("Empty Outcome", &[("Alpha", "01-first.md", None)])
                .replace("Preserve this legacy roadmap.", "   "),
        ),
    ] {
        let workspace = TempDir::new().unwrap();
        let dossier = write_dossier(workspace.path(), name, &contents);
        let result = execute_cli(
            workspace.path(),
            Operation::MigrateDossiers {
                dossier_path: Some(dossier),
                dry_run: true,
            },
        );
        assert_eq!(result.status, "ok");
        let report = result.migration_report.unwrap();
        assert_eq!(report.blocked, 1, "{:?}", report.records);
        assert!(
            report.records[0]
                .reason
                .as_deref()
                .unwrap()
                .contains("non-empty Outcome Summary"),
            "{:?}",
            report.records
        );
    }
}

#[test]
fn migration_accepts_authored_schema_aliases_without_synthesizing_metadata() {
    let workspace = TempDir::new().unwrap();
    let dossier = write_dossier(workspace.path(), "authored-schema", &authored_roadmap());

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
    let accepted = MissionStore::open(workspace.path())
        .get(report.records[0].mission_id.unwrap())
        .unwrap();
    assert_eq!(
        accepted.bundle.manifest.objective,
        "Preserve the authoring contract without adding execution metadata."
    );
    assert_eq!(accepted.bundle.waypoints[0].mode, WaypointMode::Research);
    assert_eq!(
        accepted.bundle.waypoints[1].depends_on,
        ["inspect-the-current-state"]
    );
}

#[test]
fn migration_accepts_authored_waypoints_with_an_overview_index_table() {
    let workspace = TempDir::new().unwrap();
    let roadmap = authored_roadmap().replace(
        "## Roadmap Waypoints",
        "| Waypoint | Package | Depends on | Status |\n|---|---|---|---|\n| W1 | inspect-current-state | — | done |\n| W2 | apply-change | W1 | pending |\n\n## Roadmap Waypoints",
    );
    let dossier = write_dossier(workspace.path(), "authored-with-index", &roadmap);

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: false,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.migrated, 1, "{:?}", report.records);
}

#[test]
fn migration_explains_unsupported_legacy_waypoint_index_tables() {
    let workspace = TempDir::new().unwrap();
    let dossier = write_dossier(
        workspace.path(),
        "legacy-index",
        "# ROADMAP — legacy index\n\nOutcome: Preserve legacy content.\n\n| Waypoint | Package | Depends on | Status |\n|---|---|---|---|\n| W1 | package | — | done |\n",
    );

    let result = execute_cli(
        workspace.path(),
        Operation::MigrateDossiers {
            dossier_path: Some(dossier),
            dry_run: true,
        },
    );

    assert_eq!(result.status, "ok");
    let report = result.migration_report.unwrap();
    assert_eq!(report.blocked, 1, "{:?}", report.records);
    assert!(
        report.records[0]
            .reason
            .as_deref()
            .unwrap()
            .contains("legacy waypoint index table"),
        "{:?}",
        report.records
    );
}
