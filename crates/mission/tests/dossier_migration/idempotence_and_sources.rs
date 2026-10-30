use super::*;

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
    fs::create_dir(dossier.join("inputs")).unwrap();
    fs::write(dossier.join("REQUEST.md"), "Merged request").unwrap();
    fs::write(dossier.join("inputs/one.md"), "Independent input").unwrap();

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
    let directory = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string());
    assert_eq!(
        fs::read(directory.join("REQUEST.md")).unwrap(),
        b"Merged request"
    );
    assert_eq!(
        fs::read(directory.join("inputs/one.md")).unwrap(),
        b"Independent input"
    );
    MissionStore::open(workspace.path())
        .verify_output_history(mission_id)
        .unwrap();
    let accepted = MissionStore::open(workspace.path())
        .get(mission_id)
        .unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 1);
    assert_eq!(
        accepted.bundle.manifest.artifacts,
        ["transcripts/fixture/ROADMAP.md"]
    );
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
