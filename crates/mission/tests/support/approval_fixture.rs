use mission::approval::{
    ApprovalError, EntryPath, MissionApproval, ReviewInput, ReviewKind, ReviewRecord, ReviewVerdict,
};
use mission::intake::{ManagedIntakeRequest, submit_and_create};
use mission::*;
use session_api::{RequesterDecisionRef, SessionRuntimeInitRequest, SessionStoreConfig};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};
use ticket_api::storage::store::TicketStore;
use uuid::Uuid;
pub const SESSION: &str = "11111111-1111-4111-8111-111111111111";

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
pub struct Fx {
    pub _dir: tempfile::TempDir,
    pub ws: PathBuf,
    pub session: SessionStoreConfig,
    pub id: Uuid,
    pub ticket: Uuid,
}
pub fn git_workspace() -> (tempfile::TempDir, PathBuf) {
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
    (dir, ws)
}
pub fn ready_ticket(ws: &std::path::Path) -> Uuid {
    let id = Uuid::new_v4();
    TicketStore::init(ws)
        .unwrap()
        .create(
            Some(id),
            "tracker-improvement",
            Some("Linked"),
            Some("ready"),
            BTreeMap::new(),
            None,
            Some("body"),
        )
        .unwrap();
    id
}
pub fn linked() -> Fx {
    let (dir, ws) = git_workspace();
    let ticket = ready_ticket(&ws);
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
        request_id: "w4gate",
        input: b"plan me",
    })
    .unwrap();
    let lifecycle = MissionLifecycle::open(&ws);
    let mut draft = lifecycle.get_mission_source(receipt.mission_id).unwrap();
    draft.schema_version = 3;
    draft.waypoints = vec![waypoint("a"), waypoint("b")];
    draft.waypoints[0].ownership_refs.ticket_id = Some(ticket);
    lifecycle
        .publish_draft(receipt.mission_id, draft, receipt.revision)
        .unwrap();
    Fx {
        _dir: dir,
        ws,
        session,
        id: receipt.mission_id,
        ticket,
    }
}
impl Fx {
    pub fn approval(&self) -> MissionApproval<'_> {
        MissionApproval::open(&self.ws, &self.session)
    }
    pub fn plan(&self) -> ReviewPlan {
        MissionLifecycle::open(&self.ws)
            .review_plan(self.id)
            .unwrap()
    }
    pub fn review(
        &self,
        kind: ReviewKind,
        verdict: ReviewVerdict,
        who: &str,
    ) -> Result<ReviewRecord, ApprovalError> {
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
        self.approval().record_review(
            self.id,
            ReviewInput {
                kind,
                verdict,
                reviewer: who.into(),
                revision: plan.revision,
                plan_digest: plan.plan_digest,
                evidence_identity: plan.evidence_identity,
                cold_review_id,
            },
        )
    }
    pub fn reviewed(&self) {
        self.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Pass, "cold")
            .unwrap();
        self.review(ReviewKind::Formal, ReviewVerdict::Pass, "formal")
            .unwrap();
    }
    pub fn approve(&self, challenge: &str, event: &str) -> RequesterDecisionRef {
        self.reviewed();
        let prepared = self
            .approval()
            .prepare_challenge(self.id, SESSION, challenge)
            .unwrap();
        assert!(crate::host_hook::run(&self.ws, SESSION, serde_json::json!({
            "hook_event_name": "UserPromptSubmit", "prompt": prepared.approve_line, "message_id": event,
        })).status.success());
        let reference = self
            .session
            .find_requester_decision_for_challenge(SESSION, challenge)
            .unwrap()
            .unwrap()
            .reference;
        self.approval()
            .accept_decision(self.id, &reference)
            .unwrap();
        reference
    }
    pub fn edit_b(&self, request_id: &str, prompt: &str) {
        MissionLifecycle::open(&self.ws)
            .apply_plan_operation(
                self.id,
                PlanOperationRequest {
                    request_id: request_id.into(),
                    expected_revision: self.plan().revision,
                    operation: PlanOperation::UpdateWaypoint {
                        waypoint: TypedWaypoint {
                            prompt: prompt.into(),
                            ..waypoint("b")
                        },
                    },
                },
            )
            .unwrap();
    }
    pub fn code(&self, path: EntryPath) -> &'static str {
        self.approval()
            .check_entry(self.id, path)
            .unwrap_err()
            .code()
    }
}
