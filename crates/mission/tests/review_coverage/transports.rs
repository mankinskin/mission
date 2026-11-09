use super::*;

fn named_review_args(fx: &Fx, input: &ReviewInput, request_id: &str) -> serde_json::Value {
    let mut args = serde_json::to_value(input).unwrap();
    args.as_object_mut().unwrap().remove("revision");
    args["workspace_root"] = json!(fx.ws);
    args["mission_id"] = json!(fx.id);
    args["request_id"] = json!(request_id);
    args["expected_revision"] = json!(input.revision);
    args
}

#[test]
fn named_generic_cli_and_read_projection_share_source_goal_records() {
    let fx = route(SOURCE, WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    let mut mcp = Mcp::start(&fx.ws);
    mcp.initialize();

    let cold = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold",
        None,
    );
    let named = mcp.call(
        2,
        "mission_plan_record_review",
        named_review_args(&fx, &cold, "named-cold"),
    );
    assert_eq!(named["code"], "ok", "{named}");
    let cold_id = named["result"]["review_id"].as_str().unwrap();

    let formal = input(
        &fx,
        &intent,
        ReviewKind::Formal,
        ReviewVerdict::Pass,
        "formal",
        Some(cold_id.into()),
    );
    let generic = mcp.call(
        3,
        "mission_operation",
        json!({
            "operation":"record_review",
            "mission_id":fx.id,
            "request_id":"generic-formal",
            "expected_current_revision":formal.revision,
            "review_input":formal,
        }),
    );
    assert_eq!(generic["status"], "ok", "{generic}");
    assert_eq!(generic["output_record"]["kind"], "review");

    let cli_input = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cli-cold",
        None,
    );
    let cli = cli(
        &fx.ws,
        &[
            "record-review".into(),
            fx.id.to_string(),
            "cli-cold".into(),
            serde_json::to_string(&cli_input).unwrap(),
        ],
    );
    assert_eq!(cli["status"], "ok", "{cli}");
    assert_eq!(cli["output_record"]["kind"], "review");

    let read = mcp.call(
        4,
        "mission_plan_get",
        json!({"workspace_root":fx.ws,"mission_id":fx.id}),
    );
    assert_eq!(read["code"], "ok", "{read}");
    assert_eq!(
        read["result"]["effective_intent"]["chain_sha256"],
        intent.chain_sha256
    );
    assert_eq!(read["result"]["reviews"].as_array().unwrap().len(), 3);
    assert!(read["result"]["reviews"][0]["source_goal_binding_sha256"].is_string());
    assert_eq!(
        read["result"]["reviews"][0]["goal_coverage_findings"][0]["requirement_id"],
        "search"
    );
}

#[test]
fn declared_work_kind_survives_typed_source_and_review_plan_projection() {
    let fx = route(SOURCE, WaypointWorkKind::Implementation);
    let source = MissionLifecycle::open(&fx.ws)
        .get_mission_source(fx.id)
        .unwrap();
    assert_eq!(
        source.waypoints[0].work_kind,
        Some(WaypointWorkKind::Implementation)
    );
    assert_eq!(
        fx.plan().waypoints[0].work_kind,
        Some(WaypointWorkKind::Implementation)
    );
}
