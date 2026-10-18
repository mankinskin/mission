use mission_api::*;
use std::fs;
use tempfile::tempdir;
use uuid::Uuid;

fn waypoint(id: &str, depends_on: &[&str]) -> WaypointFragment {
    WaypointFragment {
        waypoint_id: id.into(),
        title: id.into(),
        depends_on: depends_on.iter().map(|v| (*v).into()).collect(),
        mode: WaypointMode::ExecutionSideEffect,
        scope: Scope::SingleSession,
        state: Some(WaypointState::Pending),
        ticket_ref: None,
        child_mission_ref: None,
        session_package: id.into(),
        prompt: "do the work".into(),
        artifacts: vec![],
        requirement_ids: vec!["mission-one".into()],
        non_goals: vec![],
        validation: vec![ValidationGate {
            command: "cargo test".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: "checkpoint".into(),
        },
        part_markdown: "## Outcome\nDone.".into(),
    }
}
fn bundle(order: &[&str]) -> MissionBundle {
    MissionBundle {
        manifest: BundleManifest {
            schema_version: 1,
            revision: 1,
            title: "Roadmap".into(),
            objective: "Objective".into(),
            waypoint_files: vec![],
            execution_order: order.iter().map(|v| (*v).into()).collect(),
            artifacts: vec![],
            requirements: vec![Requirement {
                id: "mission-one".into(),
                text: "one".into(),
                deferred: false,
            }],
            validation_gates: vec![],
            notes: vec![],
        },
        waypoints: vec![waypoint("one", &[]), waypoint("two", &["one"])],
    }
}
fn codes(result: Result<(), Vec<Diagnostic>>) -> Vec<&'static str> {
    result.unwrap_err().into_iter().map(|d| d.code).collect()
}

#[test]
fn manifest_rejects_unknown_field() {
    let directory = tempdir().unwrap();
    let manifest = directory.path().join("manifest.json");
    fs::write(&manifest, r#"{"schema_version":1,"revision":1,"title":"a","objective":"b","waypoint_files":[],"execution_order":[],"unknown":true}"#).unwrap();
    assert!(collect_bundle(&manifest).is_err());
}
#[test]
fn manifest_rejects_unsafe_fragment_path() {
    let directory = tempdir().unwrap();
    let manifest = directory.path().join("manifest.json");
    fs::write(&manifest, r#"{"schema_version":1,"revision":1,"title":"a","objective":"b","waypoint_files":["../escape.json"],"execution_order":[]}"#).unwrap();
    assert!(
        matches!(collect_bundle(&manifest), Err(error) if error.diagnostics().unwrap()[0].code == "unsafe-fragment-path")
    );
}
#[test]
fn order_requires_each_waypoint_once() {
    assert!(
        codes(validate_bundle(&bundle(&["one"]))).contains(&"order-requires-each-waypoint-once")
    );
}
#[test]
fn order_requires_prerequisites_before_dependents() {
    assert!(
        codes(validate_bundle(&bundle(&["two", "one"])))
            .contains(&"prerequisite-must-precede-dependent")
    );
}
#[test]
fn order_requires_immediate_predecessor_chain() {
    let mut value = bundle(&["one", "two"]);
    value.waypoints[1].depends_on.clear();
    assert!(codes(validate_bundle(&value)).contains(&"missing-immediate-predecessor"));
}
#[test]
fn revision_numbers_waypoints_contiguously() {
    let rendered = render(&bundle(&["one", "two"]), Uuid::nil()).unwrap();
    let output = String::from_utf8(rendered.roadmap).unwrap();
    assert!(output.contains("### W1.") && output.contains("### W2."));
}
#[test]
fn stable_waypoint_ids_survive_reorder() {
    let mut first = bundle(&["one", "two"]);
    let rendered_first = render(&first, Uuid::nil()).unwrap();
    assert!(rendered_first.parts.contains_key("01-one.md"));
    assert!(rendered_first.parts.contains_key("02-two.md"));

    first.manifest.revision = 2;
    first.manifest.execution_order = vec!["two".into(), "one".into()];
    first.waypoints[0].depends_on = vec!["two".into()];
    first.waypoints[1].depends_on.clear();
    let rendered_second = render(&first, Uuid::nil()).unwrap();
    let roadmap = String::from_utf8(rendered_second.roadmap).unwrap();
    assert!(roadmap.contains("### W1. two (`two`)"));
    assert!(roadmap.contains("### W2. one (`one`)"));
}
#[test]
fn failed_import_preserves_accepted_revision() {
    let directory = tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let id = Uuid::new_v4();
    store.import(id, bundle(&["one", "two"]), 0).unwrap();
    let mut invalid = bundle(&["one"]);
    invalid.manifest.revision = 2;
    assert!(store.import(id, invalid, 1).is_err());
    assert_eq!(store.get(id).unwrap().bundle.manifest.revision, 1);
}
#[test]
fn invalid_waypoint_filename_id_is_rejected() {
    let mut value = bundle(&["one", "two"]);
    value.waypoints[0].waypoint_id = "../escape".into();
    assert!(codes(validate_bundle(&value)).contains(&"invalid-waypoint-id"));
}

#[test]
fn nested_missions_resolve_recursively_and_derive_child_state() {
    let directory = tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let child_id = Uuid::new_v4();
    let mut child = bundle(&["one", "two"]);
    child
        .waypoints
        .iter_mut()
        .for_each(|waypoint| waypoint.state = Some(WaypointState::Completed));
    store.import(child_id, child, 0).unwrap();

    let parent_id = Uuid::new_v4();
    let workspace = directory.path().file_name().unwrap().to_string_lossy();
    let mut parent = bundle(&["one", "two"]);
    parent.manifest.execution_order = vec!["one".into()];
    parent.waypoints.truncate(1);
    parent.waypoints[0].child_mission_ref = Some(format!("ce://{workspace}/mission/{child_id}"));
    parent.waypoints[0].state = None;
    store.import(parent_id, parent, 0).unwrap();
    assert_eq!(
        store.derived_state(parent_id).unwrap(),
        WaypointState::Completed
    );
}

#[test]
fn child_mission_rejects_missing_cross_workspace_and_cycles() {
    let directory = tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let parent_id = Uuid::new_v4();
    let mut parent = bundle(&["one", "two"]);
    parent.manifest.execution_order = vec!["one".into()];
    parent.waypoints.truncate(1);
    parent.waypoints[0].state = None;
    parent.waypoints[0].child_mission_ref = Some(format!("ce://other/mission/{parent_id}"));
    let error = store.import(parent_id, parent.clone(), 0).unwrap_err();
    assert_eq!(
        error.diagnostics().unwrap()[0].code,
        "cross-workspace-child-mission"
    );

    let workspace = directory.path().file_name().unwrap().to_string_lossy();
    parent.waypoints[0].child_mission_ref = Some(format!("ce://{workspace}/mission/{parent_id}"));
    let error = store.import(parent_id, parent, 0).unwrap_err();
    assert_eq!(error.diagnostics().unwrap()[0].code, "child-mission-cycle");
}

#[test]
fn child_mission_rejects_transitive_cycles_without_replacing_accepted_revision() {
    let directory = tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let workspace = directory.path().file_name().unwrap().to_string_lossy();
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();

    let mut child = bundle(&["one", "two"]);
    child.manifest.execution_order = vec!["one".into()];
    child.waypoints.truncate(1);
    store.import(child_id, child.clone(), 0).unwrap();

    let mut parent = bundle(&["one", "two"]);
    parent.manifest.execution_order = vec!["one".into()];
    parent.waypoints.truncate(1);
    parent.waypoints[0].state = None;
    parent.waypoints[0].child_mission_ref = Some(format!("ce://{workspace}/mission/{child_id}"));
    store.import(parent_id, parent, 0).unwrap();

    child.manifest.revision = 2;
    child.waypoints[0].state = None;
    child.waypoints[0].child_mission_ref = Some(format!("ce://{workspace}/mission/{parent_id}"));
    let error = store.import(child_id, child, 1).unwrap_err();

    assert_eq!(error.diagnostics().unwrap()[0].code, "child-mission-cycle");
    assert_eq!(store.get(child_id).unwrap().bundle.manifest.revision, 1);
}

#[test]
fn empty_mission_is_not_vacuously_complete() {
    let mut empty = bundle(&[]);
    empty.waypoints.clear();
    assert!(codes(validate_bundle(&empty)).contains(&"empty-mission"));
}

#[test]
fn unresolved_or_empty_child_preserves_published_parent() {
    let directory = tempdir().unwrap();
    let store = MissionStore::init(directory.path()).unwrap();
    let parent_id = Uuid::new_v4();
    let child_id = Uuid::new_v4();
    let original = store
        .publish(parent_id, bundle(&["one", "two"]), 0)
        .unwrap();
    let parent_path = directory
        .path()
        .join(".workflow-tools/mission/missions")
        .join(parent_id.to_string());
    let roadmap_before = std::fs::read(parent_path.join("generated/ROADMAP.md")).unwrap();
    let mut parent = bundle(&["one", "two"]);
    parent.manifest.revision = 2;
    parent.waypoints[0].state = None;
    parent.waypoints[0].child_mission_ref = Some(format!("ce://default/mission/{child_id}"));

    let missing = store.publish(parent_id, parent.clone(), 1).unwrap_err();
    assert_eq!(
        missing.diagnostics().unwrap()[0].code,
        "unaccepted-child-mission"
    );

    let mut child = store.import(child_id, bundle(&["one", "two"]), 0).unwrap();
    child.bundle.waypoints.clear();
    child.bundle.manifest.execution_order.clear();
    std::fs::write(
        directory
            .path()
            .join(".workflow-tools/mission/missions")
            .join(child_id.to_string())
            .join("accepted.json"),
        serde_json::to_vec(&child).unwrap(),
    )
    .unwrap();
    let empty = store.publish(parent_id, parent, 1).unwrap_err();
    assert_eq!(empty.diagnostics().unwrap()[0].code, "empty-child-mission");
    assert_eq!(store.get(parent_id).unwrap(), original);
    assert_eq!(
        std::fs::read(parent_path.join("generated/ROADMAP.md")).unwrap(),
        roadmap_before
    );
}
