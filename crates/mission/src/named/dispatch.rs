use super::*;
use crate::approval::MissionApproval;
use crate::{
    CancellationCapability, CompletionCapability, ControlRequest, LifecycleRead,
    PauseResumeCapability, PlanOperation, PlanOperationRequest, WaypointIdentity,
    WorkspaceLifecycle,
};

type Outcome = (Uuid, u64, Value, Value);

fn result(root: &Path, id: Uuid, result: Value) -> Result<Outcome, Error> {
    let revision = MissionLifecycle::open(root).review_plan(id)?.revision;
    Ok((id, revision, workspace(root, Some(id)), result))
}

fn plan(
    root: &Path,
    id: Uuid,
    request: String,
    expected: u64,
    operation: PlanOperation,
) -> Result<Outcome, Error> {
    nonempty(&request, "request_id")?;
    let outcome = MissionLifecycle::open(root).apply_plan_operation(
        id,
        PlanOperationRequest {
            request_id: request,
            expected_revision: expected,
            operation,
        },
    )?;
    result(root, id, value(outcome)?)
}

fn control(request: &str, sequence: u64, evidence: String) -> Result<ControlRequest, Error> {
    nonempty(&evidence, "evidence")?;
    Ok(ControlRequest {
        op_id: id(request)?,
        expected_control_sequence: sequence,
        evidence: Some(evidence),
    })
}

fn managed(receipt: crate::intake::ManagedIntake) -> Result<Outcome, Error> {
    Ok((
        receipt.mission_id,
        receipt.revision,
        json!({
            "workspace_root":receipt.workspace_root, "mission_dir":receipt.mission_dir,
            "input_path":receipt.input_path, "session_input_path":receipt.session_input_path,
            "source_path":receipt.source_path, "generated_plan_entry":receipt.generated_plan_entry.path,
            "generated_plan_state":format!("{:?}", receipt.generated_plan_entry.state),
        }),
        json!({"session_id":receipt.session_id,"request_id":receipt.request_id,
        "input_digest":receipt.input_digest,"executable":receipt.executable}),
    ))
}

pub(super) fn run(tool: &str, arguments: Value) -> Result<Outcome, Error> {
    match tool {
        "mission_intake_create" => {
            let i: IntakeInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let store = session(&root, i.session_root.as_deref())?;
            managed(crate::intake::submit_and_create(
                &crate::intake::ManagedIntakeRequest {
                    session_store: &store,
                    workspace_root: &root,
                    session_id: &i.session_id,
                    request_id: &i.request_id,
                    input: i.input.as_bytes(),
                },
            )?)
        }
        "mission_workspace_get" => {
            let i: WorkspaceInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            managed(crate::intake::find(
                &session(&root, i.session_root.as_deref())?,
                &root,
                &i.session_id,
                &i.request_id,
            )?)
        }
        "mission_plan_get" => {
            let i: ReadInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let store = session(&root, i.session_root.as_deref())?;
            let approval = MissionApproval::open(&root, &store);
            let receipts = approval
                .captured_decisions(id)?
                .into_iter()
                .filter(|r| i.session_id.as_ref().is_none_or(|sid| &r.session_id == sid))
                .collect::<Vec<_>>();
            result(
                &root,
                id,
                json!({"plan":MissionLifecycle::open(&root).review_plan(id)?,
                "lifecycle":WorkspaceLifecycle::open(&root).read_lifecycle(
                    &WaypointIdentity::Mission { mission_id:id })?,
                "reviews":approval.reviews(id)?, "grants":approval.grants(id)?,
                "pending":approval.pending(id)?.iter().map(|p| json!({
                    "challenge_id":p.challenge_id,"stage":p.stage})).collect::<Vec<_>>(),
                "decision_refs":receipts}),
            )
        }
        "mission_plan_update" => {
            let i: PlanInput = parse(arguments)?;
            plan(
                &root(&i.workspace_root)?,
                id(&i.mission_id)?,
                i.request_id,
                i.expected_revision,
                i.operation.into(),
            )
        }
        "mission_waypoint_create" => {
            let i: CreateInput = parse(arguments)?;
            plan(
                &root(&i.workspace_root)?,
                id(&i.mission_id)?,
                i.request_id,
                i.expected_revision,
                PlanOperation::CreateWaypoint {
                    waypoint: i.waypoint,
                    after: i.after,
                },
            )
        }
        "mission_waypoint_update" => {
            let i: UpdateInput = parse(arguments)?;
            plan(
                &root(&i.workspace_root)?,
                id(&i.mission_id)?,
                i.request_id,
                i.expected_revision,
                PlanOperation::UpdateWaypoint {
                    waypoint: i.waypoint,
                },
            )
        }
        "mission_waypoint_remove" => {
            let i: RemoveInput = parse(arguments)?;
            plan(
                &root(&i.workspace_root)?,
                id(&i.mission_id)?,
                i.request_id,
                i.expected_revision,
                PlanOperation::RemoveWaypoint {
                    waypoint_id: i.waypoint_id,
                    replacement: i.replacement,
                },
            )
        }
        "mission_waypoint_reorder" => {
            let i: ReorderInput = parse(arguments)?;
            plan(
                &root(&i.workspace_root)?,
                id(&i.mission_id)?,
                i.request_id,
                i.expected_revision,
                PlanOperation::ReorderWaypoints { order: i.order },
            )
        }
        "mission_plan_record_review" => {
            let i: RecordReviewInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            nonempty(&i.request_id, "request_id")?;
            let store = session(&root, i.session_root.as_deref())?;
            let review = MissionApproval::open(&root, &store)
                .with_expected_revision(i.expected_revision)
                .record_review_with_request_id(
                    id,
                    crate::approval::ReviewInput {
                        kind: i.kind,
                        verdict: i.verdict,
                        reviewer: i.reviewer,
                        revision: i.expected_revision,
                        plan_digest: i.plan_digest,
                        evidence_identity: i.evidence_identity,
                        cold_review_id: i.cold_review_id,
                    },
                    &i.request_id,
                )?;
            Ok((
                id,
                review.revision,
                workspace(&root, Some(id)),
                value(review)?,
            ))
        }
        "mission_plan_prepare_review" => {
            let i: ReviewInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            nonempty(&i.request_id, "request_id")?;
            let store = session(&root, i.session_root.as_deref())?;
            let c = MissionApproval::open(&root, &store)
                .with_expected_revision(i.expected_revision)
                .prepare_challenge(id, &i.session_id, &i.request_id)?;
            result(
                &root,
                id,
                json!({"challenge_id":c.challenge_id, "revision":c.revision,
                "plan_digest":c.plan_digest, "review_snapshot_digest":c.review_snapshot_digest,
                "host_prompt":c.approve_line, "approve_line":c.approve_line,
                "reject_line":c.reject_line,"replan_line":c.replan_line}),
            )
        }
        "mission_plan_approve" | "mission_plan_replan" => {
            let i: DecisionInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let store = session(&root, i.session_root.as_deref())?;
            // Bind retries to the Session-owned challenge identity; the provider
            // journals receipt consumption and rejects divergent decisions.
            if i.request_id != i.decision.challenge_id {
                return Err(Error::new(
                    "idempotency_conflict",
                    "request_id must equal the decision challenge_id",
                ));
            }
            let reference: session_api::RequesterDecisionRef = i.decision.into();
            let receipt = store.read_requester_decision(&reference)?;
            let expected = if tool == "mission_plan_approve" {
                session_api::DecisionKind::Approve
            } else {
                session_api::DecisionKind::Replan
            };
            if receipt.kind != expected {
                return Err(Error::new(
                    "approval_required",
                    "receipt decision does not match the named operation",
                ));
            }
            let accepted = MissionApproval::open(&root, &store)
                .with_expected_revision(i.expected_revision)
                .accept_decision(id, &reference)?;
            result(
                &root,
                id,
                json!({"kind":accepted.kind, "grant":accepted.grant,
                "replayed":accepted.replayed}),
            )
        }
        "mission_waypoint_complete" => {
            let i: CompleteInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let request = control(&i.request_id, i.expected_control_sequence, i.evidence)?;
            let snapshot = WorkspaceLifecycle::open(&root)
                .with_expected_revision(i.expected_revision)
                .complete_waypoint(id, &i.waypoint_id, &request)?;
            result(&root, id, value(snapshot)?)
        }
        "mission_pause" | "mission_resume" | "mission_cancel" | "mission_complete" => {
            let i: ControlInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let request = control(&i.request_id, i.expected_control_sequence, i.evidence)?;
            let provider =
                WorkspaceLifecycle::open(&root).with_expected_revision(i.expected_revision);
            let identity = WaypointIdentity::Mission { mission_id: id };
            let snapshot = match tool {
                "mission_pause" => provider.pause(&identity, &request)?,
                "mission_resume" => provider.resume(&identity, &request)?,
                "mission_cancel" => provider.cancel(&identity, &request)?,
                _ => provider.complete(&identity, &request)?,
            };
            result(&root, id, value(snapshot)?)
        }
        "mission_handoff" => {
            let i: HandoffInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let request = control(&i.request_id, i.expected_control_sequence, i.evidence)?;
            let snapshot = WorkspaceLifecycle::open(&root)
                .with_expected_revision(i.expected_revision)
                .handoff_session(id, &i.source_session, &i.destination_session, &request)?;
            result(&root, id, value(snapshot)?)
        }
        "mission_reopen_most_recent_completed" => {
            let i: ReopenInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let request = control(&i.request_id, i.expected_control_sequence, i.evidence)?;
            let snapshot = WorkspaceLifecycle::open(&root).reopen_most_recent_completed(
                id,
                i.expected_revision,
                i.expected_completion_sequence,
                &request,
                None,
            )?;
            result(&root, id, value(snapshot)?)
        }
        "mission_review_results_add_waypoints" => {
            let i: ResultsInput = parse(arguments)?;
            let root = root(&i.workspace_root)?;
            let id = id(&i.mission_id)?;
            let request = control(&i.request_id, i.expected_control_sequence, i.evidence)?;
            let provider =
                WorkspaceLifecycle::open(&root).with_expected_revision(i.expected_revision);
            let reviewed =
                provider.review_result(&WaypointIdentity::Mission { mission_id: id }, &request)?;
            let mut added = Vec::new();
            for addition in i.additions {
                let update = plan(
                    &root,
                    id,
                    addition.request_id,
                    addition.expected_revision,
                    PlanOperation::CreateWaypoint {
                        waypoint: addition.waypoint,
                        after: addition.after,
                    },
                );
                match update {
                    Ok((_, _, _, result)) => added.push(result),
                    Err(mut error) => {
                        error.details["persisted_review"] = value(&reviewed)?;
                        error.details["persisted_additions"] = json!(added);
                        return Err(error);
                    }
                }
            }
            result(&root, id, json!({"review":reviewed, "additions":added}))
        }
        _ => Err(Error::new(
            "invalid_request",
            "unknown named Mission operation",
        )),
    }
}
