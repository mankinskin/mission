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
    let output = render(&bundle(), Uuid::nil()).unwrap();
    let roadmap = String::from_utf8(output.roadmap).unwrap();
    let first_part = String::from_utf8(output.parts["01-first.md"].clone()).unwrap();
    assert!(roadmap.contains("execution-side-effect"));
    assert!(roadmap.contains("go \\| now"));
    assert!(first_part.contains("## Waypoint Details"));
    assert!(first_part.contains("`true`"));
    assert!(first_part.contains("## Part"));
}
#[test]
fn revision_mapping_preserves_old_label_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let id = Uuid::new_v4();
    store.import(id, bundle(), 0).unwrap();
    assert_eq!(store.revision_mapping(id, 1).unwrap().labels["W1"], "first");
}
