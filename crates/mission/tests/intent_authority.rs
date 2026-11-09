//! Synthetic host events prove transport plumbing, not real requester authority.
#[path = "support/direct_transports.rs"]
#[allow(dead_code, unused_imports)]
mod direct_transports;

use direct_transports::{Mcp, cli, json};
use mission::{intent::*, *};
use session_api::{HostPromptIngress, SessionRuntimeInitRequest, SessionStoreConfig};
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    root: TempDir,
    mission_id: Uuid,
    session: SessionStoreConfig,
    session_id: String,
    requirements: Vec<BaselineRequirementV4>,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let session = SessionStoreConfig::new(root.path().join(".workflow-tools/session"));
        let session_id = Uuid::new_v4().to_string();
        session
            .init_runtime_context(SessionRuntimeInitRequest {
                session_id: Some(session_id.clone()),
                predecessor_run_id: None,
                force_new_run: false,
            })
            .unwrap();
        let draft = MissionLifecycle::open(root.path())
            .create_draft("Search".into(), "Working search".into())
            .unwrap();
        let path = root
            .path()
            .join(".workflow-tools/mission/missions")
            .join(draft.mission_id.to_string())
            .join("input");
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("submitted-input.bin"), b"Working search").unwrap();
        Self {
            root,
            mission_id: draft.mission_id,
            session,
            session_id,
            requirements: vec![BaselineRequirementV4 {
                requirement_id: "search".into(),
                source: IntentSourceSpan {
                    source_sha256: intent_source_digest(b"Working search"),
                    start: 0,
                    end: 14,
                    excerpt: "Working search".into(),
                },
                outcome: "Search returns matching records".into(),
                category: IntentOutcomeCategory::WorkingBehavior,
                evidence_policy: IntentEvidencePolicy::ExecutableBehavior { allow_mock: false },
            }],
        }
    }
    fn reference(&self) -> IntentReceiptReference {
        IntentReceiptReference {
            session_id: self.session_id.clone(),
            challenge_id: Uuid::new_v4(),
        }
    }
    fn capture(&self, output: &IntentOutput) {
        let IntentOutput::Challenge { prompt, .. } = output else {
            panic!("expected challenge")
        };
        let ingress = HostPromptIngress::trusted_local_hook(
            "UserPromptSubmit",
            prompt.as_bytes().to_vec(),
            Some(Uuid::new_v4().to_string()),
        );
        self.session
            .ingest_intent_confirmation_prompt(&self.session_id, &ingress)
            .unwrap()
            .unwrap();
    }
}

#[test]
fn real_session_receipt_is_required_and_wrong_mission_is_rejected() {
    let f = Fixture::new();
    let reference = f.reference();
    let confirm = IntentOperation::ConfirmBaseline {
        expected_revision: 1,
        requirements: f.requirements.clone(),
        reference: reference.clone(),
    };
    assert!(execute_intent(f.root.path(), f.mission_id, confirm.clone()).is_err());
    let (_, challenge) = execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::PrepareBaseline {
            expected_revision: 1,
            requirements: f.requirements.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    f.capture(&challenge);
    let (_, confirmed) = execute_intent(f.root.path(), f.mission_id, confirm).unwrap();
    assert!(matches!(confirmed, IntentOutput::Baseline { .. }));
    let provider = SessionIntentProvider::new(f.root.path()).unwrap();
    assert!(
        provider
            .confirmed_receipt(Uuid::new_v4(), &reference)
            .is_err()
    );
    assert!(provider.confirmed_receipt(f.mission_id, &reference).is_ok());
}

#[test]
fn cli_generic_and_named_mcp_share_effective_intent_and_strict_inputs() {
    let f = Fixture::new();
    let reference = f.reference();
    let (_, challenge) = execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::PrepareBaseline {
            expected_revision: 1,
            requirements: f.requirements.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    f.capture(&challenge);
    execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::ConfirmBaseline {
            expected_revision: 1,
            requirements: f.requirements.clone(),
            reference,
        },
    )
    .unwrap();
    let cli_output = cli(
        f.root.path(),
        &[
            "intent".into(),
            f.mission_id.to_string(),
            json!({"action":"get"}).to_string(),
        ],
    );
    let mut mcp = Mcp::start(f.root.path());
    mcp.initialize();
    let call = |mcp: &mut Mcp, id, name: &str, arguments| {
        let response = mcp.request(id, "tools/call", json!({"name":name,"arguments":arguments}));
        serde_json::from_str::<serde_json::Value>(response["content"][0]["text"].as_str().unwrap())
            .unwrap()
    };
    let generic = call(
        &mut mcp,
        2,
        "mission_operation",
        json!({"operation":"intent","mission_id":f.mission_id,"intent_action":{"action":"get"}}),
    );
    let named = call(
        &mut mcp,
        3,
        "mission_intent",
        json!({"workspace_root":f.root.path(),"mission_id":f.mission_id,"operation":{"action":"get"}}),
    );
    assert_eq!(
        cli_output["output_record"]["record"]["result"],
        generic["output_record"]["record"]["result"]
    );
    assert_eq!(
        named["result"],
        generic["output_record"]["record"]["result"]
    );
    let rejected = call(
        &mut mcp,
        4,
        "mission_intent",
        json!({"workspace_root":f.root.path(),"mission_id":f.mission_id,"operation":{"action":"get","approved":true}}),
    );
    assert_ne!(rejected["code"], "ok");
}

#[test]
fn confirmed_amendment_excludes_requirement_without_changing_baseline() {
    let f = Fixture::new();
    let reference = f.reference();
    let (_, challenge) = execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::PrepareBaseline {
            expected_revision: 1,
            requirements: f.requirements.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    f.capture(&challenge);
    execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::ConfirmBaseline {
            expected_revision: 1,
            requirements: f.requirements.clone(),
            reference,
        },
    )
    .unwrap();
    let provider = SessionIntentProvider::new(f.root.path()).unwrap();
    let before = MissionStore::open(f.root.path())
        .effective_intent(f.mission_id, &provider)
        .unwrap()
        .unwrap();
    let reference = f.reference();
    let changes = vec![IntentChangeV4::Exclude {
        prior_requirement_id: "search".into(),
    }];
    let (_, challenge) = execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::PrepareAmendment {
            expected_revision: 1,
            amendment_id: "design-only".into(),
            prior_chain_sha256: before.chain_sha256.clone(),
            source: "I authorize excluding search implementation.".into(),
            changes: changes.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    f.capture(&challenge);
    execute_intent(
        f.root.path(),
        f.mission_id,
        IntentOperation::AppendAmendment {
            expected_revision: 1,
            amendment_id: "design-only".into(),
            prior_chain_sha256: before.chain_sha256,
            source: "I authorize excluding search implementation.".into(),
            changes,
            reference,
        },
    )
    .unwrap();
    let after = MissionStore::open(f.root.path())
        .effective_intent(f.mission_id, &provider)
        .unwrap()
        .unwrap();
    assert_eq!(before.baseline, after.baseline);
    assert!(after.requirements.is_empty());
}
