use super::*;

#[test]
fn stdio_explicit_roots_intake_replay_finished_session_and_structured_errors() {
    let (_dir, workspace, sid) = fixture();
    let mut mcp = Mcp::start(&workspace);
    mcp.initialize();
    let args = json!({"workspace_root":workspace,"session_id":sid,
        "request_id":"intake","input":"Keep exact source"});
    let first = call(&mut mcp, 2, "mission_intake_create", args.clone());
    assert_eq!(first["code"], "ok", "{first}");
    let replay = call(&mut mcp, 3, "mission_intake_create", args.clone());
    assert_eq!(replay["mission_id"], first["mission_id"]);
    let located = call(
        &mut mcp,
        4,
        "mission_workspace_get",
        json!({
        "workspace_root":workspace,"session_id":sid,"request_id":"intake"}),
    );
    assert_eq!(located["workspace"], first["workspace"]);
    let mut divergent = args;
    divergent["input"] = json!("Changed bytes");
    assert_eq!(
        call(&mut mcp, 5, "mission_intake_create", divergent)["code"],
        "idempotency_conflict"
    );
    let missing = call(
        &mut mcp,
        6,
        "mission_workspace_get",
        json!({
        "workspace_root":workspace,"session_id":sid,"request_id":"absent"}),
    );
    assert_eq!(missing["code"], "intake_missing", "{missing}");
    let invalid = call(
        &mut mcp,
        7,
        "mission_plan_get",
        json!({"mission_id":first["mission_id"]}),
    );
    assert_eq!(invalid["code"], "invalid_request", "{invalid}");
    let unavailable = call(
        &mut mcp,
        8,
        "mission_plan_get",
        json!({
        "workspace_root":workspace.join("absent"),"mission_id":first["mission_id"]}),
    );
    assert_eq!(
        unavailable["code"], "workspace_unavailable",
        "{unavailable}"
    );
    SessionStoreConfig::new(workspace.join(".workflow-tools/session"))
        .finish_workflow(&sid, vec![], vec![], None)
        .unwrap();
    let finished = call(
        &mut mcp,
        9,
        "mission_intake_create",
        json!({
        "workspace_root":workspace,"session_id":sid,"request_id":"after-finish","input":"Cannot create"}),
    );
    assert_eq!(finished["code"], "session_finished", "{finished}");
}

#[test]
fn stdio_waypoint_edits_requirements_and_idempotency_delegate_to_planning() {
    let (_dir, workspace, sid) = fixture();
    let mut mcp = Mcp::start(&workspace);
    mcp.initialize();
    let intake = call(
        &mut mcp,
        2,
        "mission_intake_create",
        json!({
        "workspace_root":workspace,"session_id":sid,"request_id":"planning","input":"Plan"}),
    );
    let id = &intake["mission_id"];
    let mut revision = intake["revision"].as_u64().unwrap();
    let mut apply = |rpc, name, mut payload: serde_json::Value| {
        payload["workspace_root"] = json!(workspace);
        payload["mission_id"] = id.clone();
        payload["expected_revision"] = json!(revision);
        let out = call(&mut mcp, rpc, name, payload);
        assert_eq!(out["code"], "ok", "{out}");
        revision = out["revision"].as_u64().unwrap();
    };
    apply(
        3,
        "mission_waypoint_create",
        json!({"request_id":"create-a","waypoint":waypoint("a")}),
    );
    apply(
        4,
        "mission_waypoint_create",
        json!({"request_id":"create-b","waypoint":waypoint("b")}),
    );
    let mut b = waypoint("b");
    b.prompt = "Updated prompt".into();
    apply(
        5,
        "mission_waypoint_update",
        json!({"request_id":"update-b","waypoint":b}),
    );
    apply(
        6,
        "mission_waypoint_reorder",
        json!({"request_id":"order","order":["b","a"]}),
    );
    apply(
        7,
        "mission_waypoint_remove",
        json!({"request_id":"remove-a","waypoint_id":"a"}),
    );
    apply(
        8,
        "mission_plan_update",
        json!({"request_id":"requirements","operation":{
        "op":"set_requirements","requirements":[{
            "requirement":{"id":"req-one","text":"Named edits retain identity"},
            "owner_waypoint_ids":["b"]}]}}),
    );
    let read = call(
        &mut mcp,
        9,
        "mission_plan_get",
        json!({"workspace_root":workspace,"mission_id":id}),
    );
    assert_eq!(
        read["result"]["plan"]["waypoints"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        read["result"]["plan"]["waypoints"][0]["prompt"],
        "Updated prompt"
    );
    assert_eq!(read["result"]["plan"]["requirements"][0]["id"], "req-one");
    let absent = call(
        &mut mcp,
        10,
        "mission_waypoint_remove",
        json!({
        "workspace_root":workspace,"mission_id":id,"request_id":"absent",
        "expected_revision":revision,"waypoint_id":"absent"}),
    );
    assert_eq!(absent["code"], "waypoint_not_found", "{absent}");
    let request = json!({"workspace_root":workspace,"mission_id":id,"request_id":"one-goal",
        "expected_revision":revision,"operation":{"op":"set_goal","title":"Goal","objective":"One goal"}});
    let goal = call(&mut mcp, 11, "mission_plan_update", request.clone());
    assert_eq!(goal["code"], "ok", "{goal}");
    let replay = call(&mut mcp, 12, "mission_plan_update", request.clone());
    assert_eq!(replay["code"], "ok", "{replay}");
    assert_eq!(replay["result"]["replayed"], true);
    let mut conflict = request;
    conflict["operation"]["title"] = json!("Different");
    assert_eq!(
        call(&mut mcp, 13, "mission_plan_update", conflict)["code"],
        "idempotency_conflict"
    );
}

#[test]
fn stdio_receipt_kind_and_missing_authority_fail_closed() {
    let f = approval_fixture::linked();
    let mut mcp = Mcp::start(&f.ws);
    mcp.initialize();
    f.reviewed();
    let prepared = call(
        &mut mcp,
        2,
        "mission_plan_prepare_review",
        json!({
        "workspace_root":f.ws,"mission_id":f.id,"request_id":"replan-kind",
        "expected_revision":f.plan().revision,"session_id":approval_fixture::SESSION}),
    );
    assert_eq!(prepared["code"], "ok", "{prepared}");
    assert!(
        host_hook::run(
            &f.ws,
            approval_fixture::SESSION,
            json!({
        "hook_event_name":"UserPromptSubmit","prompt":prepared["result"]["replan_line"],
        "message_id":"replan-event"})
        )
        .status
        .success()
    );
    let reference = f
        .session
        .find_requester_decision_for_challenge(approval_fixture::SESSION, "replan-kind")
        .unwrap()
        .unwrap()
        .reference;
    let args = json!({"workspace_root":f.ws,"mission_id":f.id,"request_id":"replan-kind",
        "expected_revision":f.plan().revision,"decision":reference});
    let reject = call(&mut mcp, 3, "mission_plan_approve", args.clone());
    assert_eq!(reject["code"], "approval_required", "{reject}");
    let replan = call(&mut mcp, 4, "mission_plan_replan", args.clone());
    assert_eq!(replan["code"], "ok", "{replan}");
    assert_eq!(replan["result"]["kind"], "replan");
    assert!(replan["result"]["grant"].is_null());
    let mut missing = args;
    missing["decision"]["decision_id"] = json!("missing");
    let unavailable = call(&mut mcp, 5, "mission_plan_replan", missing);
    assert_eq!(
        unavailable["code"], "approval_authority_unavailable",
        "{unavailable}"
    );
    assert!(f.approval().grants(f.id).unwrap().is_empty());
}
