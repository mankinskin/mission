use super::*;

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
