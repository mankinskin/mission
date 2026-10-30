//! W4 reviewed requester-bound approval. W8 owns real installed-host evidence.
#[path = "support/host_hook.rs"]
mod host_hook;
use mission::approval::{
    ApprovalError, ApprovalStep, EntryPath, MissionApproval, ReviewInput, ReviewKind, ReviewVerdict,
};
use mission::intake::{ManagedIntakeRequest, submit_and_create};
use mission::*;
use session_api::{
    DecisionKind, RequesterDecisionRef, SessionRuntimeInitRequest, SessionStoreConfig,
};
use std::{fs, path::PathBuf, process::Command};
use uuid::Uuid;

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
struct Fx {
    _dir: tempfile::TempDir,
    ws: PathBuf,
    session: SessionStoreConfig,
    id: Uuid,
}
fn fx() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path().join("ws");
    fs::create_dir_all(&ws).unwrap();
    for args in [
        vec!["init", "--quiet"],
        vec!["config", "user.email", "h@e.com"],
        vec!["config", "user.name", "h"],
        vec!["commit", "--quiet", "--allow-empty", "-m", "i"],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&ws)
                .status()
                .unwrap()
                .success()
        );
    }
    let session = SessionStoreConfig::new(ws.join(".workflow-tools/session"));
    session
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(SESSION.into()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    let receipt = submit_and_create(&ManagedIntakeRequest {
        session_store: &session,
        workspace_root: &ws,
        session_id: SESSION,
        request_id: "w4",
        input: b"plan me",
    })
    .unwrap();
    let lifecycle = MissionLifecycle::open(&ws);
    let mut draft = lifecycle.get_mission_source(receipt.mission_id).unwrap();
    draft.schema_version = 3;
    draft.waypoints = vec![waypoint("a"), waypoint("b")];
    lifecycle
        .publish_draft(receipt.mission_id, draft, receipt.revision)
        .unwrap();
    Fx {
        _dir: dir,
        ws,
        session,
        id: receipt.mission_id,
    }
}
impl Fx {
    fn approval(&self) -> MissionApproval<'_> {
        MissionApproval::open(&self.ws, &self.session)
    }
    fn plan(&self) -> ReviewPlan {
        MissionLifecycle::open(&self.ws)
            .review_plan(self.id)
            .unwrap()
    }
    fn review(
        &self,
        kind: ReviewKind,
        verdict: ReviewVerdict,
        who: &str,
    ) -> Result<(), ApprovalError> {
        let plan = self.plan();
        let cold_review_id = if kind == ReviewKind::Formal {
            self.approval()
                .reviews(self.id)?
                .iter()
                .rev()
                .find(|r| {
                    r.kind == ReviewKind::ColdWalkthrough
                        && r.verdict == ReviewVerdict::Pass
                        && r.revision == plan.revision
                })
                .map(|r| r.review_id.clone())
        } else {
            None
        };
        self.approval()
            .record_review(
                self.id,
                ReviewInput {
                    kind,
                    verdict,
                    reviewer: who.into(),
                    revision: plan.revision,
                    plan_digest: plan.plan_digest.clone(),
                    evidence_identity: plan.evidence_identity.clone(),
                    cold_review_id,
                },
            )
            .map(|_| ())
    }
    fn reviewed(&self) {
        self.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Pass, "cold")
            .unwrap();
        self.review(ReviewKind::Formal, ReviewVerdict::Pass, "formal")
            .unwrap();
    }
    fn edit(&self, request_id: &str) {
        let lifecycle = MissionLifecycle::open(&self.ws);
        lifecycle
            .apply_plan_operation(
                self.id,
                PlanOperationRequest {
                    request_id: request_id.into(),
                    expected_revision: self.plan().revision,
                    operation: PlanOperation::UpdateWaypoint {
                        waypoint: TypedWaypoint {
                            prompt: format!("{request_id} changed"),
                            ..waypoint("a")
                        },
                    },
                },
            )
            .unwrap();
    }
    fn challenge_line(&self, challenge: &str, kind: DecisionKind) -> String {
        let prepared = self
            .approval()
            .prepare_challenge(self.id, SESSION, challenge)
            .unwrap();
        match kind {
            DecisionKind::Approve => prepared.approve_line,
            DecisionKind::Reject => prepared.reject_line,
            DecisionKind::Replan => prepared.replan_line,
        }
    }
    fn submit(&self, line: &str, event_id: Option<&str>) -> std::process::Output {
        let mut payload =
            serde_json::json!({"hook_event_name": "UserPromptSubmit", "prompt": line});
        if let Some(id) = event_id {
            payload["message_id"] = id.into();
        }
        host_hook::run(&self.ws, SESSION, payload)
    }
    fn decision_ref(&self, challenge: &str) -> RequesterDecisionRef {
        self.session
            .find_requester_decision_for_challenge(SESSION, challenge)
            .unwrap()
            .expect("receipt")
            .reference
    }
    fn grant_via_hook(&self, challenge: &str) -> RequesterDecisionRef {
        let line = self.challenge_line(challenge, DecisionKind::Approve);
        assert!(self.submit(&line, Some("host-evt")).status.success());
        self.decision_ref(challenge)
    }
    fn code(&self, path: EntryPath) -> &'static str {
        self.approval()
            .check_entry(self.id, path)
            .unwrap_err()
            .code()
    }
}

#[test]
fn review_requires_cold_then_distinct_formal_on_current_digest() {
    let f = fx();
    assert_eq!(
        f.review(ReviewKind::Formal, ReviewVerdict::Pass, "formal")
            .unwrap_err()
            .code(),
        "approval_required"
    );
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c0")
            .unwrap_err()
            .code(),
        "approval_required"
    );
    f.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Pass, "cold")
        .unwrap();
    assert_eq!(
        f.review(ReviewKind::Formal, ReviewVerdict::Pass, "cold")
            .unwrap_err()
            .code(),
        "approval_review_not_distinct"
    );
    f.review(ReviewKind::Formal, ReviewVerdict::Fail, "formal")
        .unwrap();
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c0")
            .unwrap_err()
            .code(),
        "approval_required"
    );
    f.review(ReviewKind::Formal, ReviewVerdict::Pass, "formal")
        .unwrap();
    let stale = f.plan();
    f.edit("e1");
    let error = f
        .approval()
        .record_review(
            f.id,
            ReviewInput {
                kind: ReviewKind::Formal,
                verdict: ReviewVerdict::Pass,
                reviewer: "formal".into(),
                revision: stale.revision,
                plan_digest: stale.plan_digest,
                evidence_identity: stale.evidence_identity,
                cold_review_id: None,
            },
        )
        .unwrap_err();
    assert_eq!(error.code(), "approval_stale");
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c1")
            .unwrap_err()
            .code(),
        "approval_required"
    );
}

#[test]
fn real_hook_decision_becomes_one_grant_and_replays_identically() {
    let f = fx();
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
    f.reviewed();
    let reference = f.grant_via_hook("c1");
    assert_eq!(reference.source_event_id, "host-evt");
    let accepted = f.approval().accept_decision(f.id, &reference).unwrap();
    let grant = accepted.grant.clone().expect("approve grants");
    assert_eq!(grant.plan_digest, f.plan().plan_digest);
    assert_eq!(grant.decision, reference);
    assert!(!accepted.replayed);
    let replay = f.approval().accept_decision(f.id, &reference).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.grant, accepted.grant);
    assert_eq!(f.approval().grants(f.id).unwrap().len(), 1);
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert_eq!(f.approval().check_entry(f.id, path).unwrap(), grant);
    }
    assert!(
        f.submit(
            &f.approval()
                .prepare_challenge(f.id, SESSION, "c1")
                .unwrap()
                .approve_line,
            Some("host-evt")
        )
        .status
        .success()
    );
    assert_eq!(f.decision_ref("c1"), reference);
    assert_eq!(f.approval().grants(f.id).unwrap().len(), 1);
}

#[test]
fn missing_host_event_id_is_allocated_and_still_verifies() {
    let f = fx();
    f.reviewed();
    let line = f.challenge_line("c1", DecisionKind::Approve);
    assert!(f.submit(&line, None).status.success());
    let reference = f.decision_ref("c1");
    assert!(!reference.source_event_id.is_empty());
    f.approval()
        .accept_decision(f.id, &reference)
        .unwrap()
        .grant
        .unwrap();
}

#[test]
fn generic_reactions_actor_text_and_forged_sources_never_authorize() {
    let f = fx();
    f.reviewed();
    let plan = f.plan();
    let json = serde_json::json!({
        "revision": plan.revision, "decision": "approve", "signature": "sha256:x",
        "actor": "requester", "actor_source": "agent text"
    });
    let _ = MissionStore::open(&f.ws).record_approval_event_json(f.id, json.to_string().as_bytes());
    assert_eq!(f.code(EntryPath::Dispatch), "approval_required");
    let line = f.challenge_line("c1", DecisionKind::Approve);
    for name in ["Stop", "PostToolUse"] {
        let _ = host_hook::run(
            &f.ws,
            SESSION,
            serde_json::json!({"hook_event_name": name, "prompt": line}),
        );
    }
    assert!(
        f.session
            .find_requester_decision_for_challenge(SESSION, "c1")
            .unwrap()
            .is_none()
    );
    let forged = RequesterDecisionRef {
        session_id: SESSION.into(),
        challenge_id: "c1".into(),
        decision_id: "forged".into(),
        source_event_id: "forged".into(),
    };
    assert_eq!(
        f.approval()
            .accept_decision(f.id, &forged)
            .unwrap_err()
            .code(),
        "approval_authority_unavailable"
    );
    assert!(f.approval().grants(f.id).unwrap().is_empty());
}

#[test]
fn reject_and_replan_decisions_never_grant() {
    for (kind, challenge) in [(DecisionKind::Reject, "r1"), (DecisionKind::Replan, "p1")] {
        let f = fx();
        f.reviewed();
        let line = f.challenge_line(challenge, kind);
        assert!(f.submit(&line, Some("e")).status.success());
        let accepted = f
            .approval()
            .accept_decision(f.id, &f.decision_ref(challenge))
            .unwrap();
        assert_eq!(accepted.kind, kind);
        assert!(accepted.grant.is_none());
        assert_eq!(f.code(EntryPath::Activation), "approval_required");
    }
}

#[test]
fn semantic_edit_makes_grant_stale_but_evidence_and_regeneration_do_not() {
    let f = fx();
    f.reviewed();
    let reference = f.grant_via_hook("c1");
    f.approval().accept_decision(f.id, &reference).unwrap();
    let lifecycle = MissionLifecycle::open(&f.ws);
    let plan = f.plan();
    lifecycle
        .apply_plan_operation(
            f.id,
            PlanOperationRequest {
                request_id: "ev".into(),
                expected_revision: plan.revision,
                operation: PlanOperation::RecordEvidence {
                    plan_digest: plan.plan_digest.clone(),
                    result: OperatorResult {
                        waypoint_id: "a".into(),
                        outcome: "passed".into(),
                        operator_command: "cargo test".into(),
                        evidence_pointer: "ce://default/test/receipt-a".into(),
                    },
                },
            },
        )
        .unwrap();
    assert!(f.plan().revision > plan.revision);
    let grant = f.approval().check_entry(f.id, EntryPath::Resume).unwrap();
    assert_eq!(grant.plan_digest, plan.plan_digest);
    lifecycle.review_plan(f.id).unwrap();
    f.approval().check_entry(f.id, EntryPath::Dispatch).unwrap();
    f.edit("semantic");
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert_eq!(f.code(path), "approval_stale");
    }
}

#[test]
fn interrupted_acceptance_is_pending_then_resumes_exactly_once() {
    let f = fx();
    f.reviewed();
    let reference = f.grant_via_hook("c1");
    let error = f
        .approval()
        .accept_decision_with_fault(f.id, &reference, Some(ApprovalStep::BeforeGrant))
        .unwrap_err();
    assert_eq!(error.code(), "approval_pending");
    let pending = f.approval().pending(f.id).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].challenge_id, reference.challenge_id);
    assert_eq!(pending[0].stage, "decision-unaccepted");
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
    let error = f
        .approval()
        .accept_decision_with_fault(f.id, &reference, Some(ApprovalStep::AfterGrant))
        .unwrap_err();
    assert_eq!(error.code(), "approval_pending");
    let done = f.approval().accept_decision(f.id, &reference).unwrap();
    assert!(done.grant.is_some());
    assert!(f.approval().pending(f.id).unwrap().is_empty());
    assert_eq!(f.approval().grants(f.id).unwrap().len(), 1);
}

#[test]
fn divergent_second_decision_on_consumed_challenge_conflicts() {
    let f = fx();
    f.reviewed();
    let reference = f.grant_via_hook("c1");
    f.approval().accept_decision(f.id, &reference).unwrap();
    let reject = f
        .approval()
        .prepare_challenge(f.id, SESSION, "c1")
        .unwrap()
        .reject_line;
    let _ = f.submit(&reject, Some("other"));
    assert_eq!(f.decision_ref("c1"), reference);
    assert_eq!(
        f.approval()
            .accept_decision(f.id, &reference)
            .unwrap()
            .grant
            .unwrap()
            .decision,
        reference
    );
}

#[test]
fn finished_session_is_historical_only() {
    let f = fx();
    f.reviewed();
    let line = f.challenge_line("c1", DecisionKind::Approve);
    assert!(f.submit(&line, Some("e")).status.success());
    let reference = f.decision_ref("c1");
    f.session
        .finish_workflow(SESSION, vec![], vec![], None)
        .unwrap();
    assert!(f.session.read_requester_decision(&reference).is_ok());
    assert!(f.approval().accept_decision(f.id, &reference).is_err());
    assert!(f.approval().grants(f.id).unwrap().is_empty());
    assert!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "fresh")
            .is_err()
    );
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
}
