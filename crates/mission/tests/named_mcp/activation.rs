use super::{approval_fixture::linked, call, direct_transports::Mcp};
use mission::{LifecycleRead, WaypointIdentity, WorkspaceLifecycle};
use serde_json::{Value, json};
use uuid::Uuid;

fn arguments(f: &super::approval_fixture::Fx) -> Value {
    json!({
        "workspace_root": f.ws, "mission_id": f.id, "waypoint_id": "a",
        "request_id": Uuid::new_v4(), "expected_revision": f.plan().revision,
        "author": "activation-test"
    })
}

fn native(f: &super::approval_fixture::Fx) -> String {
    WorkspaceLifecycle::open(&f.ws)
        .read_lifecycle(&WaypointIdentity::Ticket {
            ticket_id: f.ticket,
        })
        .unwrap()
        .native_state
}

#[test]
fn activation_cli_exposes_guarded_ticket_transition() {
    let f = linked();
    let args = arguments(&f);
    let run = || {
        let output = super::direct_transports::cli_output(
            &f.ws,
            Some(&f.ws),
            None,
            &[
                "activate-waypoint".into(),
                f.id.to_string(),
                "a".into(),
                "--request-id".into(),
                args["request_id"].as_str().unwrap().into(),
                "--expected-revision".into(),
                f.plan().revision.to_string(),
                "--author".into(),
                "activation-test".into(),
            ],
        );
        let envelope: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)));
        assert_eq!(output.status.success(), envelope["code"] == "ok");
        envelope
    };
    assert_eq!(run()["code"], "approval_required");
    assert_eq!(native(&f), "ready");
    f.approve("activate-cli", "activate-cli-event");
    let active = run();
    assert_eq!(active["code"], "ok", "{active}");
    assert_eq!(active["result"]["native_state"], "in-implementation");
    assert_eq!(native(&f), "in-implementation");
    f.edit_b("activate-stale", "Changed plan");
    assert_eq!(run()["code"], "approval_stale");
}

#[test]
fn activation_mcp_exposes_guarded_ticket_transition() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let args = arguments(&f);
    let denied = call(&mut mcp, 2, "mission_waypoint_activate", args.clone());
    assert_eq!(denied["code"], "approval_required", "{denied}");
    assert_eq!(native(&f), "ready");
    f.approve("activate-mcp", "activate-mcp-event");
    let mut stale = args.clone();
    stale["expected_revision"] = json!(f.plan().revision - 1);
    assert_eq!(
        call(&mut mcp, 3, "mission_waypoint_activate", stale)["code"],
        "revision_conflict"
    );
    assert_eq!(native(&f), "ready");
    let active = call(&mut mcp, 4, "mission_waypoint_activate", args.clone());
    assert_eq!(active["code"], "ok", "{active}");
    assert_eq!(active["result"]["native_state"], "in-implementation");
    assert_eq!(native(&f), "in-implementation");
    f.edit_b("activate-stale", "Changed plan");
    let mut fresh = args;
    fresh["expected_revision"] = json!(f.plan().revision);
    assert_eq!(
        call(&mut mcp, 5, "mission_waypoint_activate", fresh)["code"],
        "approval_stale"
    );
}

#[test]
fn activation_rejects_invalid_inputs_and_unsupported_waypoints_without_ticket_writes() {
    let f = linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let args = arguments(&f);
    let mut rpc = 2;
    for field in [
        "mission_id",
        "waypoint_id",
        "request_id",
        "expected_revision",
    ] {
        let mut missing = args.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert_eq!(
            call(&mut mcp, rpc, "mission_waypoint_activate", missing)["code"],
            "invalid_request"
        );
        rpc += 1;
    }
    for (field, invalid) in [
        ("mission_id", json!("not-a-uuid")),
        ("mission_id", json!(Uuid::nil())),
        ("request_id", json!(Uuid::nil())),
        ("request_id", json!("not-a-uuid")),
        ("waypoint_id", json!(" ")),
        ("author", json!(" ")),
        ("expected_revision", json!(-1)),
        ("to_native_state", json!("done")),
    ] {
        let mut bad = args.clone();
        bad[field] = invalid;
        assert_eq!(
            call(&mut mcp, rpc, "mission_waypoint_activate", bad)["code"],
            "invalid_request"
        );
        rpc += 1;
    }
    let mut unsupported = args;
    unsupported["waypoint_id"] = json!("b");
    assert_eq!(
        call(&mut mcp, rpc, "mission_waypoint_activate", unsupported)["code"],
        "unsupported_capability"
    );
    assert_eq!(native(&f), "ready");
}

#[test]
fn activation_checks_waypoint_prerequisites_before_ticket_transition() {
    let f = linked();
    let lifecycle = mission::MissionLifecycle::open(&f.ws);
    let mut draft = lifecycle.get_mission_source(f.id).unwrap();
    draft.waypoints[0].ownership_refs.ticket_id = None;
    draft.waypoints[1].ownership_refs.ticket_id = Some(f.ticket);
    lifecycle
        .publish_draft(f.id, draft, f.plan().revision)
        .unwrap();
    f.approve("prerequisite", "prerequisite-event");
    let mut args = arguments(&f);
    args["waypoint_id"] = json!("b");
    args["author"] = Value::Null;
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    let denied = call(&mut mcp, 2, "mission_waypoint_activate", args);
    assert_eq!(denied["code"], "prerequisite_evidence_required", "{denied}");
    assert_eq!(native(&f), "ready");
}
