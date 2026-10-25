use mission::{
    CommitCheckpoint, DirectWaypoint, MissionLifecycle, MigrationState, ValidationGate,
};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

fn legacy(root: &TempDir) -> uuid::Uuid {
    MissionLifecycle::open(root.path()).create(
        "legacy".into(), "keep objective".into(),
        vec![DirectWaypoint {
            waypoint_id: "one".into(), title: "One".into(),
            prompt: "independent prompt".into(),
            depends_on: vec![],
            validation: vec![ValidationGate { command: "cargo test".into(), cwd: None }],
            commit_checkpoint: CommitCheckpoint { message: "test: migration".into() },
            non_goals: vec!["none".into()],
        }],
    ).unwrap().mission_id
}

#[test]
fn migration_preserves_history_and_makes_typed_source_authoritative() {
    let root = TempDir::new().unwrap();
    let id = legacy(&root);
    let dir = root.path().join(".workflow-tools/mission/missions").join(id.to_string());
    let before = fs::read(dir.join("history/revision-1.json")).unwrap();
    let mut journal = MissionLifecycle::plan_migration(&[root.path().to_path_buf()]).unwrap();
    assert_eq!(journal.records.len(), 1);
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Applied);
    let lifecycle = MissionLifecycle::open(root.path());
    let source = lifecycle.get_mission_source(id).unwrap();
    assert_eq!(source.revision, 2);
    assert_eq!(source.waypoints[0].prompt, "independent prompt");
    assert_eq!(fs::read(dir.join("history/revision-1.json")).unwrap(), before);
    assert!(dir.join("history/revision-2.json").is_file());
    assert!(dir.join("parts/one.json").is_file());
}

#[test]
fn migration_stops_on_cas_drift_and_rolls_back_only_owned_apply() {
    let root = TempDir::new().unwrap();
    let id = legacy(&root);
    let mut journal = MissionLifecycle::plan_migration(&[root.path().to_path_buf()]).unwrap();
    MissionLifecycle::open(root.path()).edit_objective(id, "changed".into(), 1).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Failed);
    assert!(journal.records[0].diagnostic.as_deref().unwrap().contains("CAS"));

    let root = TempDir::new().unwrap();
    let id = legacy(&root);
    let mut journal = MissionLifecycle::plan_migration(&[PathBuf::from(root.path())]).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    MissionLifecycle::rollback_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::RolledBack);
    assert!(MissionLifecycle::open(root.path()).get_mission_source(id).is_err());
}

#[test]
fn migration_reports_semantic_or_formatting_diagnostics_for_unsupported_markdown() {
    use mission::MissionError;

    let root = TempDir::new().unwrap();
    MissionLifecycle::open(root.path()).create(
        "legacy".into(), "keep objective".into(),
        vec![DirectWaypoint {
            waypoint_id: "one".into(), title: "One".into(),
            prompt: "<div>unsupported</div>".into(),
            depends_on: vec![],
            validation: vec![ValidationGate { command: "cargo test".into(), cwd: None }],
            commit_checkpoint: CommitCheckpoint { message: "test: migration".into() },
            non_goals: vec!["none".into()],
        }],
    ).unwrap();
    let error = MissionLifecycle::plan_migration(&[root.path().to_path_buf()]).unwrap_err();
    match error {
        MissionError::Validation(_, diagnostics) => {
            assert_eq!(diagnostics[0].code, "semantic_or_formatting");
            assert_eq!(diagnostics[0].pointer, "/waypoints/one/part_markdown/line/1");
        }
        other => panic!("expected validation diagnostic, got {other:?}"),
    }
}
