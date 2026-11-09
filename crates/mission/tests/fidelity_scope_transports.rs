#[allow(dead_code, clippy::double_ended_iterator_last)]
#[path = "support/host_hook.rs"]
mod host_hook;
#[allow(dead_code)]
#[path = "fidelity_scope/support.rs"]
mod support;

#[cfg(all(feature = "cli", feature = "mcp"))]
#[allow(dead_code, unused_imports)]
#[path = "support/direct_transports.rs"]
mod direct_transports;

#[cfg(all(feature = "cli", feature = "mcp"))]
#[test]
fn cli_named_and_generic_review_routes_preserve_scope_findings_identically() {
    use mission::{
        IntentOutcomeCategory, WaypointWorkKind,
        approval::{ReviewKind, ReviewVerdict},
    };

    let fx = support::contract_route(WaypointWorkKind::Implementation);
    let intent = support::confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    let input = support::scoped_input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold reviewer",
        None,
    );
    let request_id = "scope-transport";
    let cli = direct_transports::cli(
        &fx.ws,
        &[
            "record-review".into(),
            fx.id.to_string(),
            request_id.into(),
            serde_json::to_string(&input).unwrap(),
        ],
    );
    assert_eq!(cli["status"], "ok", "{cli}");
    let cli_review_id = cli["output_record"]["record"]["review"]["review_id"]
        .as_str()
        .unwrap();

    let mut mcp = direct_transports::Mcp::start(&fx.ws);
    mcp.initialize();
    let tools = mcp.request(2, "tools/list", serde_json::json!({}));
    let named_schema = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "mission_plan_record_review")
        .unwrap();
    assert!(
        named_schema["inputSchema"]
            .to_string()
            .contains("scope_findings")
    );
    let mut named_input = serde_json::to_value(&input).unwrap();
    named_input["workspace_root"] = serde_json::json!(fx.ws);
    named_input["mission_id"] = serde_json::json!(fx.id);
    named_input["request_id"] = serde_json::json!(request_id);
    named_input["expected_revision"] = serde_json::json!(input.revision);
    named_input.as_object_mut().unwrap().remove("revision");
    let named = mcp.call(3, "mission_plan_record_review", named_input);
    assert_eq!(named["code"], "ok", "{named}");
    assert_eq!(named["result"]["review_id"], cli_review_id);

    let generic = mcp.call(
        4,
        "mission_operation",
        serde_json::json!({
            "operation":"record_review",
            "mission_id":fx.id,
            "request_id":request_id,
            "expected_current_revision":input.revision,
            "review_input":input,
        }),
    );
    assert_eq!(generic["status"], "ok", "{generic}");
    assert_eq!(
        generic["output_record"]["record"]["review"]["review_id"],
        cli_review_id
    );
}
