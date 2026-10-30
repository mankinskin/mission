use mission_api::{MissionError, MissionLifecycle};
use uuid::Uuid;

#[test]
fn create_draft_with_id_uses_the_supplied_identity_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(dir.path());
    let id = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap();
    let draft = lifecycle
        .create_draft_with_id(id, "Title".into(), "Objective".into())
        .unwrap();
    assert_eq!(draft.mission_id, id);
    assert_eq!(draft.revision, 1);
    assert_eq!(lifecycle.get_mission_source(id).unwrap(), draft);
    assert!(
        dir.path()
            .join(".workflow-tools/mission/missions")
            .join(id.to_string())
            .join("source.json")
            .is_file()
    );
    let error = lifecycle
        .create_draft_with_id(id, "Other".into(), "Other".into())
        .unwrap_err();
    assert!(
        matches!(
            error,
            MissionError::StaleRevision {
                expected: 0,
                actual: 1
            }
        ),
        "{error:?}"
    );
    assert_eq!(lifecycle.get_mission_source(id).unwrap(), draft);
}
