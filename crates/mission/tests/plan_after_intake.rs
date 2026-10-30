//! Real W2 intake followed by W3 planning, retaining immutable provenance.
use mission::intake::{ManagedIntakeRequest, find, submit_and_create};
use mission_api::*;
use serde_json::Value;
use session_api::{SessionRuntimeInitRequest, SessionStoreConfig};
use std::{collections::BTreeMap, fs, path::Path};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";

fn waypoint(id: &str) -> TypedWaypoint {
    TypedWaypoint {
        waypoint_id: id.into(),
        title: format!("{id} title"),
        prompt: format!("{id} prompt"),
        part: PartSource {
            waypoint_id: id.into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: format!("{id} body"),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "cargo test".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: format!("{id} checkpoint"),
        },
        non_goals: vec![format!("{id} non-goal")],
        outcome_plan: Some(WaypointOutcomePlan {
            expected_outcome: format!("{id} is ready"),
            alternative_outcomes: vec![AlternativeOutcome {
                outcome: format!("{id} is blocked"),
                acceptable_handling: vec!["Inspect the failure".into()],
            }],
        }),
        ..Default::default()
    }
}
fn input_files(input_dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(input_dir)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_file())
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                fs::read(e.path()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn intake_created_mission_is_planned_by_operations_with_provenance_and_replay_intact() {
    let dir = tempfile::tempdir().unwrap();
    let session_root = dir.path().join("session-store");
    let workspace = dir.path().join("ws");
    fs::create_dir_all(&workspace).unwrap();
    let session = SessionStoreConfig::new(&session_root);
    session
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(SESSION.to_string()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    let bytes: &[u8] = b"\xff\x00 raw \r\n bytes\r\n";
    let request = ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &workspace,
        session_id: SESSION,
        request_id: "w3-entry",
        input: bytes,
    };
    let receipt = submit_and_create(&request).unwrap();
    let id = receipt.mission_id;
    let input_dir = receipt.input_path.parent().unwrap().to_path_buf();
    let provenance = input_files(&input_dir);
    assert_eq!(
        provenance
            .values()
            .filter(|b| b.as_slice() == bytes)
            .count(),
        1
    );
    let lifecycle = MissionLifecycle::open(&workspace);
    let mut draft = lifecycle.get_mission_source(id).unwrap();
    draft.schema_version = 3;
    draft.waypoints = vec![waypoint("a"), waypoint("b")];
    lifecycle
        .publish_draft(id, draft, receipt.revision)
        .unwrap();
    let mut revision = receipt.revision + 1;
    let mut apply = |request_id: &str, operation: PlanOperation| {
        let outcome = lifecycle
            .apply_plan_operation(
                id,
                PlanOperationRequest {
                    request_id: request_id.into(),
                    expected_revision: revision,
                    operation,
                },
            )
            .unwrap();
        assert_eq!(outcome.revision, revision + 1);
        revision = outcome.revision;
        outcome
    };
    apply(
        "title",
        PlanOperation::SetGoal {
            title: "Retitled".into(),
            objective: "New goal".into(),
        },
    );
    apply(
        "order",
        PlanOperation::ReorderWaypoints {
            order: vec!["b".into(), "a".into()],
        },
    );
    apply(
        "reqs",
        PlanOperation::SetRequirements {
            requirements: vec![PlanRequirement {
                requirement: Requirement {
                    id: "req-one".into(),
                    text: "one".into(),
                    deferred: false,
                    canonical_criteria: vec![],
                },
                owner_waypoint_ids: vec!["a".into()],
            }],
        },
    );
    let digest = serde_json::to_value(lifecycle.review_plan(id).unwrap()).unwrap()["plan_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    apply(
        "evidence",
        PlanOperation::RecordEvidence {
            plan_digest: digest,
            result: OperatorResult {
                waypoint_id: "a".into(),
                outcome: "passed".into(),
                operator_command: "cargo test".into(),
                evidence_pointer: "ce://default/test/receipt-a".into(),
            },
        },
    );
    let review: Value = serde_json::to_value(lifecycle.review_plan(id).unwrap()).unwrap();
    let order: Vec<&str> = review["waypoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["waypoint_id"].as_str().unwrap())
        .collect();
    assert_eq!(order, ["b", "a"]);
    assert_eq!(review["title"], "Retitled");
    assert_eq!(input_files(&input_dir), provenance);
    assert_eq!(fs::read(&receipt.session_input_path).unwrap(), bytes);
    let found = find(&session, &workspace, SESSION, "w3-entry").unwrap();
    assert_eq!(found.mission_id, id);
    assert_eq!(found.input_digest, receipt.input_digest);
    assert_eq!(found.revision, revision);
    let replay = submit_and_create(&request).unwrap();
    assert_eq!(replay.mission_id, id);
    assert_eq!(input_files(&input_dir), provenance);
}
