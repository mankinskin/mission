use std::{
    fs,
    path::{Path, PathBuf},
};

use mission::{MissionStore, Operation, execute_cli};
use tempfile::TempDir;

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
