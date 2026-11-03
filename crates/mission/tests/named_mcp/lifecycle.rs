use super::*;
use approval_and_completion::{approve, control, evidence};
use approval_fixture::{SESSION, linked};
use ticket_api::storage::store::TicketStore;

fn completed(f: &approval_fixture::Fx, mcp: &mut Mcp) {
    let a = evidence(f, mcp, "a", 2);
    let b = evidence(f, mcp, "b", 3);
    approve(f, mcp, "w6-all", 4);
    TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("w6-test"))
        .unwrap();
    for (rpc, id, pointer) in [(7, "a", a), (8, "b", b)] {
        let mut args = control(f, &pointer);
        args["waypoint_id"] = json!(id);
        let out = call(mcp, rpc, "mission_waypoint_complete", args);
        assert_eq!(out["code"], "ok", "{out}");
    }
}

#[test]
fn stdio_result_review_mission_completion_and_latest_reopen_preserve_guards() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let mut empty_reopen = control(&f, "no completion");
    empty_reopen["expected_completion_sequence"] = json!(1);
    assert_eq!(
        call(
            &mut mcp,
            20,
            "mission_reopen_most_recent_completed",
            empty_reopen
        )["code"],
        "no_completed_waypoint"
    );
    completed(&f, &mut mcp);
    let premature = call(
        &mut mcp,
        9,
        "mission_complete",
        control(&f, "review required"),
    );
    assert_eq!(premature["code"], "result_review_required", "{premature}");
    let mut review = control(&f, "reviewed all outcomes");
    review["additions"] = json!([]);
    let reviewed = call(&mut mcp, 10, "mission_review_results_add_waypoints", review);
    assert_eq!(reviewed["code"], "ok", "{reviewed}");
    let done = call(
        &mut mcp,
        11,
        "mission_complete",
        control(&f, "accepted all outcomes"),
    );
    assert_eq!(done["code"], "ok", "{done}");
    assert_eq!(done["result"]["state"], "completed");
    let mut reopen = control(&f, "review most recent completion");
    reopen["expected_completion_sequence"] = json!(2);
    let reopened = call(&mut mcp, 12, "mission_reopen_most_recent_completed", reopen);
    assert_eq!(reopened["code"], "ok", "{reopened}");
    assert_eq!(reopened["result"]["identity"]["waypoint_id"], "b");
    assert_eq!(reopened["result"]["state"], "planned");
    let before = mission::MissionLifecycle::open(&f.ws)
        .begin_approval_transaction(f.id)
        .unwrap()
        .control_state()
        .unwrap();
    assert_eq!(before.completions.len(), 2);
    assert!(!before.completions[1].current);
    assert_eq!(
        f.code(mission::approval::EntryPath::Dispatch),
        "approval_stale"
    );
}

#[test]
fn stdio_result_review_addition_is_semantic_and_partial_failure_is_explicit() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    completed(&f, &mut mcp);
    let mut extra = mission::MissionLifecycle::open(&f.ws)
        .get_mission_source(f.id)
        .unwrap()
        .waypoints[1]
        .clone();
    extra.waypoint_id = "c".into();
    extra.part.waypoint_id = "c".into();
    let mut review = control(&f, "add reviewed work");
    review["additions"] = json!([{
        "request_id":"add-c","expected_revision":f.plan().revision,
        "waypoint":extra,"after":"b"
    }]);
    let added = call(&mut mcp, 9, "mission_review_results_add_waypoints", review);
    assert_eq!(added["code"], "ok", "{added}");
    assert_eq!(f.plan().waypoints.len(), 3);
    assert_eq!(
        f.code(mission::approval::EntryPath::Dispatch),
        "approval_stale"
    );

    let g = linked();
    let mut other = Mcp::start(&g.ws);
    other.initialize();
    completed(&g, &mut other);
    let mut fail = control(&g, "review persisted but addition is stale");
    fail["additions"] = json!([{
        "request_id":"stale-c","expected_revision":0,"waypoint":waypoint("c"),"after":null
    }]);
    let partial = call(&mut other, 9, "mission_review_results_add_waypoints", fail);
    assert_eq!(partial["code"], "revision_conflict", "{partial}");
    assert!(partial["details"]["persisted_review"].is_object());
    assert_eq!(partial["details"]["persisted_additions"], json!([]));
    assert_eq!(g.plan().waypoints.len(), 2);
}

#[test]
fn stdio_handoff_and_cancel_keep_session_and_ticket_authority() {
    const DEST: &str = "22222222-2222-4222-8222-222222222222";
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    approve(&f, &mut mcp, "w6-handoff", 2);
    f.session
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(DEST.into()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    assert!(
        host_hook::run(
            &f.ws,
            DEST,
            json!({
                "hook_event_name":"UserPromptSubmit","prompt":"Continue","message_id":"destination"
            })
        )
        .status
        .success()
    );
    let mut handoff = control(&f, "transfer binding");
    handoff["source_session"] = json!(SESSION);
    handoff["destination_session"] = json!(DEST);
    let missing_pin = call(&mut mcp, 50, "mission_handoff", handoff.clone());
    assert_ne!(missing_pin["code"], "ok", "{missing_pin}");
    let urn = format!("ce://default/mission/{}", f.id);
    f.session
        .pin_runtime_entity(
            SESSION,
            &urn,
            Some("w6-fixture".into()),
            Some("handoff source".into()),
        )
        .unwrap();
    let out = call(&mut mcp, 5, "mission_handoff", handoff);
    assert_eq!(out["code"], "ok", "{out}");
    assert!(
        f.session
            .read_runtime_context(DEST)
            .unwrap()
            .pinned_entities
            .iter()
            .any(|p| p.urn == urn)
    );
    assert!(
        !f.session
            .read_runtime_context(SESSION)
            .unwrap()
            .pinned_entities
            .iter()
            .any(|p| p.urn == urn)
    );
    let cancel = call(
        &mut mcp,
        6,
        "mission_cancel",
        control(&f, "requester cancelled"),
    );
    assert_eq!(cancel["code"], "ok", "{cancel}");
    assert_eq!(cancel["result"]["state"], "cancelled");
    assert_eq!(
        TicketStore::open(&f.ws)
            .unwrap()
            .get(&f.ticket)
            .unwrap()
            .extra["state"],
        "ready"
    );
}
