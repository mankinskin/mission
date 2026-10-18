use mission_api::*;
use uuid::Uuid;

fn bundle() -> MissionBundle {
    let waypoint = |id: &str, depends_on: Vec<String>, title: &str| WaypointFragment {
        waypoint_id: id.into(),
        title: title.into(),
        depends_on,
        mode: WaypointMode::ExecutionSideEffect,
        scope: Scope::SingleSession,
        state: Some(WaypointState::Pending),
        ticket_ref: None,
        child_mission_ref: None,
        session_package: id.into(),
        prompt: "go | now".into(),
        artifacts: vec![],
        requirement_ids: vec![],
        non_goals: vec![],
        validation: vec![ValidationGate {
            command: "true".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "ok".into(),
        },
        part_markdown: "## Part".into(),
    };
    MissionBundle {
        manifest: BundleManifest {
            schema_version: 1,
            revision: 1,
            title: "A | B".into(),
            objective: "objective".into(),
            waypoint_files: vec![],
            execution_order: vec!["first".into(), "second".into()],
            artifacts: vec![],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
        },
        waypoints: vec![
            waypoint("first", vec![], "First | waypoint"),
            waypoint("second", vec!["first".into()], "Second"),
        ],
    }
}
#[test]
fn render_is_byte_deterministic() {
    assert_eq!(
        render(&bundle(), Uuid::nil()).unwrap(),
        render(&bundle(), Uuid::nil()).unwrap()
    );
}
#[test]
fn render_escapes_pipes_and_resolves_links() {
    let output = String::from_utf8(render(&bundle(), Uuid::nil()).unwrap().roadmap).unwrap();
    assert!(output.contains("A \\| B") && output.contains("(01-first.md)"));
}
#[test]
fn render_parts_follow_execution_order() {
    let output = render(&bundle(), Uuid::nil()).unwrap();
    assert_eq!(output.parts.keys().next().unwrap(), "01-first.md");
    assert!(
        String::from_utf8(output.parts["01-first.md"].clone())
            .unwrap()
            .contains("Next: [02-second.md](02-second.md)")
    );
}
#[test]
fn render_contains_waypoint_contract_and_validation_details() {
    let mut value = bundle();
    value.waypoints[0].state = None;
    value.waypoints[0].child_mission_ref = Some(format!("ce://workspace/mission/{}", Uuid::nil()));
    let output = render(&value, Uuid::nil()).unwrap();
    let roadmap = String::from_utf8(output.roadmap).unwrap();
    let first_part = String::from_utf8(output.parts["01-first.md"].clone()).unwrap();
    assert!(roadmap.contains("execution-side-effect"));
    assert!(roadmap.contains("go \\| now"));
    assert!(first_part.contains("## Waypoint Details"));
    assert!(first_part.contains("`true`"));
    assert!(first_part.contains("## Part"));
    assert!(
        roadmap.contains("**Child mission**")
            && first_part.contains("ce://workspace/mission/00000000-0000-0000-0000-000000000000")
    );
}
#[test]
fn revision_mapping_preserves_old_label_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let id = Uuid::new_v4();
    store.import(id, bundle(), 0).unwrap();
    assert_eq!(store.revision_mapping(id, 1).unwrap().labels["W1"], "first");
}

#[test]
fn render_with_snapshot_accepts_ephemeral_external_state() {
    let mut value = bundle();
    let ticket_ref = format!("ce://workspace/ticket/{}", Uuid::nil());
    value.waypoints[0].scope = Scope::TicketBacked {
        ticket_ref: ticket_ref.clone(),
    };
    value.waypoints[0].ticket_ref = Some(ticket_ref);
    value.waypoints[0].state = Some(WaypointState::InProgress);

    assert!(render(&value, Uuid::nil()).is_err());
    let output =
        render_with_snapshot(&value, Uuid::nil(), Some("ticket-status-v1:fixture")).unwrap();
    let roadmap = String::from_utf8(output.roadmap).unwrap();

    assert!(roadmap.contains("external-status-snapshot: ticket-status-v1:fixture"));
    assert!(roadmap.contains("| **State** | in-progress |"));
}

#[test]
fn validation_rejects_conflicting_ticket_references() {
    let mut value = bundle();
    value.waypoints[0].scope = Scope::TicketBacked {
        ticket_ref: format!("ce://workspace/ticket/{}", Uuid::nil()),
    };
    value.waypoints[0].ticket_ref = Some(format!("ce://workspace/ticket/{}", Uuid::new_v4()));
    value.waypoints[0].state = None;

    assert!(
        validate_bundle(&value)
            .unwrap_err()
            .iter()
            .any(|diagnostic| diagnostic.code == "ticket-reference-mismatch")
    );
}

#[test]
fn snapshot_derivation_keeps_unstarted_local_waypoints_pending() {
    let directory = tempfile::tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let id = Uuid::new_v4();
    let mut value = bundle();
    value.waypoints[0].state = None;
    value.waypoints[1].state = Some(WaypointState::Blocked);
    store.import(id, value, 0).unwrap();

    assert_eq!(
        store
            .derived_state_with_status_snapshot(id, &Default::default())
            .unwrap(),
        WaypointState::Pending
    );

    let mut parent = bundle();
    parent.waypoints[0].state = None;
    parent.waypoints[0].child_mission_ref = Some(format!("ce://default/mission/{id}"));
    parent.waypoints[1].state = Some(WaypointState::Completed);
    let parent_id = Uuid::new_v4();
    store.import(parent_id, parent, 0).unwrap();
    assert_eq!(
        store
            .derived_state_with_status_snapshot(parent_id, &Default::default())
            .unwrap(),
        WaypointState::Pending
    );
}
