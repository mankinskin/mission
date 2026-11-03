use super::*;
use approval_fixture::{SESSION, linked};
use mission::{LifecycleRead, WaypointIdentity, WorkspaceLifecycle};
use ticket_api::storage::store::TicketStore;
use uuid::Uuid;

pub(super) fn control(f: &approval_fixture::Fx, evidence: &str) -> serde_json::Value {
    json!({
        "workspace_root":f.ws, "mission_id":f.id, "request_id":Uuid::new_v4(),
        "expected_revision":f.plan().revision,
        "expected_control_sequence":WorkspaceLifecycle::open(&f.ws)
            .read_lifecycle(&WaypointIdentity::Mission { mission_id:f.id }).unwrap().control_sequence,
        "evidence":evidence,
    })
}

pub(super) fn evidence(
    f: &approval_fixture::Fx,
    mcp: &mut Mcp,
    waypoint: &str,
    rpc: u64,
) -> String {
    let pointer = format!("ce://default/test/{waypoint}-result");
    let outcome = call(
        mcp,
        rpc,
        "mission_plan_update",
        json!({
            "workspace_root":f.ws, "mission_id":f.id,
            "request_id":format!("evidence-{waypoint}"), "expected_revision":f.plan().revision,
            "operation":{"op":"record_evidence", "plan_digest":f.plan().plan_digest,
                "result":{"waypoint_id":waypoint,"outcome":"passed",
                    "operator_command":"true","evidence_pointer":pointer}}
        }),
    );
    assert_eq!(outcome["code"], "ok", "{outcome}");
    pointer
}

pub(super) fn approve(
    f: &approval_fixture::Fx,
    mcp: &mut Mcp,
    challenge: &str,
    rpc: u64,
) -> serde_json::Value {
    f.reviewed();
    let prepared = call(
        mcp,
        rpc,
        "mission_plan_prepare_review",
        json!({
            "workspace_root":f.ws, "mission_id":f.id, "session_id":SESSION,
            "request_id":challenge, "expected_revision":f.plan().revision
        }),
    );
    assert_eq!(prepared["code"], "ok", "{prepared}");
    assert_eq!(prepared["result"]["challenge_id"], challenge);
    let prompt = prepared["result"]["host_prompt"].as_str().unwrap();
    assert!(prompt.starts_with("mission-decision approve "));
    assert!(
        host_hook::run(
            &f.ws,
            SESSION,
            json!({
                "hook_event_name":"UserPromptSubmit","prompt":prompt,
                "message_id":format!("{challenge}-event")
            })
        )
        .status
        .success()
    );
    let read = call(
        mcp,
        rpc + 1,
        "mission_plan_get",
        json!({
            "workspace_root":f.ws, "mission_id":f.id
        }),
    );
    assert_eq!(read["code"], "ok", "{read}");
    let reference = f
        .session
        .find_requester_decision_for_challenge(SESSION, challenge)
        .unwrap()
        .unwrap()
        .reference;
    assert!(
        read["result"]["decision_refs"]
            .as_array()
            .unwrap()
            .contains(&json!(reference))
    );
    let args = json!({"workspace_root":f.ws,"mission_id":f.id,
        "request_id":challenge,"expected_revision":f.plan().revision,"decision":reference});
    let approved = call(mcp, rpc + 2, "mission_plan_approve", args.clone());
    assert_eq!(approved["code"], "ok", "{approved}");
    assert_eq!(approved["result"]["kind"], "approve");
    args
}

#[test]
fn stdio_receipt_approval_replay_and_fresh_binding_after_semantic_replan() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let args = approve(&f, &mut mcp, "w6-first", 2);
    let replay = call(&mut mcp, 5, "mission_plan_approve", args.clone());
    assert_eq!(replay["code"], "ok", "{replay}");
    assert_eq!(replay["result"]["replayed"], true);
    let fake = call(
        &mut mcp,
        6,
        "mission_plan_approve",
        json!({
            "workspace_root":f.ws, "mission_id":f.id,"request_id":"fake",
            "expected_revision":f.plan().revision,"actor":"requester","approve":true
        }),
    );
    assert_eq!(fake["code"], "invalid_request", "{fake}");

    let edited = call(
        &mut mcp,
        7,
        "mission_plan_update",
        json!({
            "workspace_root":f.ws,"mission_id":f.id,"request_id":"changed-goal",
            "expected_revision":f.plan().revision,
            "operation":{"op":"set_goal","title":"Changed","objective":"Fresh requester decision"}
        }),
    );
    assert_eq!(edited["code"], "ok", "{edited}");
    let stale = call(&mut mcp, 8, "mission_plan_approve", args);
    assert_eq!(stale["code"], "revision_conflict", "{stale}");
    let resume = call(&mut mcp, 9, "mission_resume", control(&f, "not reapproved"));
    assert_eq!(resume["code"], "approval_stale", "{resume}");
    let prepared_old = call(
        &mut mcp,
        10,
        "mission_plan_prepare_review",
        json!({
            "workspace_root":f.ws,"mission_id":f.id,"request_id":"w6-first",
            "expected_revision":f.plan().revision,"session_id":SESSION
        }),
    );
    assert_ne!(prepared_old["code"], "ok", "{prepared_old}");
    approve(&f, &mut mcp, "w6-new", 11);
    assert!(
        f.approval()
            .check_entry(f.id, mission::approval::EntryPath::Dispatch)
            .is_ok()
    );
}

#[test]
fn stdio_completion_preserves_approval_evidence_ticket_cas_and_provider_replay() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let pointer = evidence(&f, &mut mcp, "a", 2);
    let mut args = control(&f, &pointer);
    args["waypoint_id"] = json!("a");
    let denied = call(&mut mcp, 3, "mission_waypoint_complete", args.clone());
    assert_eq!(denied["code"], "approval_required", "{denied}");
    approve(&f, &mut mcp, "w6-complete", 4);
    let ticket_denied = call(&mut mcp, 7, "mission_waypoint_complete", args.clone());
    assert_eq!(
        ticket_denied["code"], "ticket_transition_rejected",
        "{ticket_denied}"
    );
    TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("w6-test"))
        .unwrap();

    let mut empty = args.clone();
    empty["evidence"] = json!("");
    assert_eq!(
        call(&mut mcp, 8, "mission_waypoint_complete", empty)["code"],
        "invalid_request"
    );
    let mut invalid_id = args.clone();
    invalid_id["request_id"] = json!("not-a-uuid");
    assert_eq!(
        call(&mut mcp, 9, "mission_waypoint_complete", invalid_id)["code"],
        "invalid_request"
    );
    let mut stale_revision = args.clone();
    stale_revision["expected_revision"] = json!(f.plan().revision - 1);
    assert_eq!(
        call(&mut mcp, 10, "mission_waypoint_complete", stale_revision)["code"],
        "revision_conflict"
    );
    let mut stale_seq = args.clone();
    stale_seq["expected_control_sequence"] = json!(999);
    assert_eq!(
        call(&mut mcp, 11, "mission_waypoint_complete", stale_seq)["code"],
        "revision_conflict"
    );
    let mut absent = args.clone();
    absent["waypoint_id"] = json!("absent");
    assert_eq!(
        call(&mut mcp, 12, "mission_waypoint_complete", absent)["code"],
        "waypoint_not_found"
    );
    let mut old_evidence = args.clone();
    old_evidence["evidence"] = json!("ce://default/test/unbound");
    assert_eq!(
        call(&mut mcp, 13, "mission_waypoint_complete", old_evidence)["code"],
        "completion_evidence_required"
    );

    let completed = call(&mut mcp, 14, "mission_waypoint_complete", args.clone());
    assert_eq!(completed["code"], "ok", "{completed}");
    assert_eq!(completed["result"]["state"], "completed");
    let replay = call(&mut mcp, 15, "mission_waypoint_complete", args);
    assert_eq!(replay["code"], "revision_conflict", "{replay}");
    assert_eq!(replay["details"]["provider_code"], "stale_control_sequence");
    let state = mission::MissionLifecycle::open(&f.ws)
        .begin_approval_transaction(f.id)
        .unwrap()
        .control_state()
        .unwrap();
    assert_eq!(state.completions.len(), 1);
    assert_eq!(state.operations.len(), 1);
}

#[test]
fn stdio_pause_denies_completion_and_preserves_ticket_authority() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let pointer = evidence(&f, &mut mcp, "a", 2);
    approve(&f, &mut mcp, "w6-paused", 3);
    let paused = call(&mut mcp, 6, "mission_pause", control(&f, "inspection"));
    assert_eq!(paused["code"], "ok", "{paused}");
    assert_eq!(
        TicketStore::open(&f.ws)
            .unwrap()
            .get(&f.ticket)
            .unwrap()
            .extra["state"],
        "ready"
    );
    let mut args = control(&f, &pointer);
    args["waypoint_id"] = json!("a");
    let denied = call(&mut mcp, 7, "mission_waypoint_complete", args);
    assert_ne!(denied["code"], "ok", "{denied}");
    assert_eq!(
        mission::MissionLifecycle::open(&f.ws)
            .begin_approval_transaction(f.id)
            .unwrap()
            .control_state()
            .unwrap()
            .completions
            .len(),
        0
    );
    let resume = call(
        &mut mcp,
        8,
        "mission_resume",
        control(&f, "approved resume"),
    );
    assert_eq!(resume["code"], "ok", "{resume}");
}
