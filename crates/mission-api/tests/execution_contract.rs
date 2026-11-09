use mission_api::{
    CONTRACT_BUNDLE_SCHEMA_VERSION, CONTRACT_SOURCE_SCHEMA_VERSION, MissionLifecycle, TypedDraft,
    WaypointExecutionContractV4, render, waypoint_execution_contract_digest,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn fixture() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(".execution-contract-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
}

fn v4_source() -> TypedDraft {
    serde_json::from_str(include_str!("fixtures/execution-contract-v4.json")).unwrap()
}

#[test]
fn v4_contract_round_trips_through_store_review_and_rendering() {
    let fixture = fixture();
    let mut source = v4_source();
    let mission_id = source.mission_id;
    let lifecycle = MissionLifecycle::open(fixture.path());
    lifecycle
        .create_draft_with_id(mission_id, source.title.clone(), source.objective.clone())
        .unwrap();
    source.revision = 1;
    let published = lifecycle.publish_draft(mission_id, source, 1).unwrap();

    let loaded_source = lifecycle.get_mission_source(mission_id).unwrap();
    assert_eq!(loaded_source.schema_version, CONTRACT_SOURCE_SCHEMA_VERSION);
    assert_eq!(loaded_source, published);
    let accepted = lifecycle.get(mission_id).unwrap();
    assert_eq!(
        accepted.bundle.manifest.schema_version,
        CONTRACT_BUNDLE_SCHEMA_VERSION
    );
    assert_eq!(
        accepted.bundle.waypoints[0].execution_contract,
        loaded_source.waypoints[0].execution_contract
    );

    let contract = loaded_source.waypoints[0]
        .execution_contract
        .as_ref()
        .unwrap();
    let expected_digest = waypoint_execution_contract_digest(contract).unwrap();
    let review = lifecycle.review_plan(mission_id).unwrap();
    assert_eq!(
        review.waypoints[0].execution_contract_digest.as_deref(),
        Some(expected_digest.as_str())
    );
    assert_eq!(
        review.waypoints[0].execution_contract.as_ref(),
        Some(contract)
    );

    let typed_render = lifecycle.render_mission(mission_id).unwrap();
    assert!(typed_render.contains("Execution contract (declared; not executed by Mission)"));
    let generated = render(&accepted.bundle, mission_id).unwrap();
    let roadmap = String::from_utf8(generated.roadmap).unwrap();
    assert!(roadmap.contains("renderer: mission-api/8"));
    assert!(roadmap.contains("Execution contract (declared; not executed by Mission)"));
    let part = String::from_utf8(generated.parts["waypoints/01-w1.md"].clone()).unwrap();
    assert!(part.contains("Authoritative context"));
    assert!(part.contains("Failure handling"));

    let mut changed_source = loaded_source;
    changed_source.waypoints[0]
        .execution_contract
        .as_mut()
        .unwrap()
        .rationale = "A changed semantic boundary.".into();
    let updated = lifecycle
        .publish_draft(mission_id, changed_source, published.revision)
        .unwrap();
    let changed_review = lifecycle.review_plan(mission_id).unwrap();
    assert_eq!(updated.revision, published.revision + 1);
    assert_ne!(changed_review.plan_digest, review.plan_digest);
}

#[test]
fn every_contract_semantic_field_changes_its_canonical_digest() {
    let source = v4_source();
    let value =
        serde_json::to_value(source.waypoints[0].execution_contract.as_ref().unwrap()).unwrap();
    let original = waypoint_execution_contract_digest(
        source.waypoints[0].execution_contract.as_ref().unwrap(),
    )
    .unwrap();
    let changes = [
        ("schema_version", json!(5)),
        ("outcome_category", json!("artifact")),
        ("expected_outcome", json!("Changed outcome")),
        ("work_kind", json!("documentation")),
        ("rationale", json!("Changed rationale")),
        (
            "authoritative_context",
            json!([{"id":"context-spec","reference":"ce://default/spec/changed","description":"EA-01"}]),
        ),
        (
            "boundary",
            json!({"task":"ticket:other","paths":["workflow-tools/mission-api"]}),
        ),
        ("declared_dependency_ids", json!(["w0"])),
        (
            "no_dependencies_reason",
            json!("A changed explicit reason."),
        ),
        (
            "preconditions",
            json!([{"id":"pre-source","description":"Changed condition","check_ids":["check-preflight"]}]),
        ),
        (
            "postconditions",
            json!([{"id":"post-behavior","description":"Changed condition","check_ids":["check-acceptance"]}]),
        ),
        (
            "checks",
            json!([
                {"id":"check-preflight","command":"changed","cwd":".","kind":"preflight","evidence_class":"deterministic_artifact"},
                {"id":"check-acceptance","command":"cargo test","cwd":".","kind":"acceptance","evidence_class":"executable_behavior"},
                {"id":"check-regression","command":"cargo test","cwd":".","kind":"regression","evidence_class":"executable_behavior"}
            ]),
        ),
        (
            "evidence_requirements",
            json!([{"id":"evidence-test","class":"deterministic_artifact","check_ids":["check-preflight"]}]),
        ),
        ("regression_check_ids", json!([])),
        (
            "no_regression_checks_reason",
            json!("No regressions apply."),
        ),
        (
            "tool_policy",
            json!({"actions":["filesystem_read","cli_read_only"],"read_roots":["workflow-tools"],"write_roots":[],"permissions":["read","execute"],"no_write_reason":"No writes are required."}),
        ),
        (
            "environment",
            json!({"profile":"other-profile","description":"Different environment","preflight_check_ids":["check-preflight"]}),
        ),
        (
            "failure_policy",
            json!({"trigger":"Different trigger","diagnostic_action":"Inspect","max_attempts":2,"exhaustion":"escalate","escalation_target":"Owner"}),
        ),
        (
            "handover",
            json!({"id":"handover","producer":{"waypoint_id":"w1","artifact_id":"artifact","content_reference":"api.json","schema":"v1","root":"workflow-tools","check_ids":["check-acceptance"]},"consumer":{"waypoint_id":"w2","artifact_id":"artifact","content_reference":"api.json","schema":"v1","root":"workflow-tools","check_ids":["check-acceptance"]}}),
        ),
        ("no_handover_reason", json!("A different terminal route.")),
        (
            "provider_route",
            json!({"id":"provider","provider":"host","version":"1","receipt_class":"readiness","preflight_check_ids":["check-preflight"]}),
        ),
        (
            "no_provider_reason",
            json!("A different provider decision."),
        ),
    ];
    for (field, changed_value) in changes {
        let mut changed = value.clone();
        changed[field] = changed_value;
        let changed: WaypointExecutionContractV4 = serde_json::from_value(changed).unwrap();
        assert_ne!(
            waypoint_execution_contract_digest(&changed).unwrap(),
            original,
            "changing semantic field {field} must change the digest"
        );
    }
}

#[test]
fn v2_v3_sources_omit_new_contract_fields_and_keep_legacy_rendering() {
    let bytes = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "tests/fixtures/representative-missions/authoring/mission-integration.typed-v3.json",
    ))
    .unwrap();
    let legacy: TypedDraft = serde_json::from_slice(&bytes).unwrap();
    let serialized = serde_json::to_value(&legacy).unwrap();
    assert_eq!(legacy.schema_version, 3);
    assert!(
        legacy
            .waypoints
            .iter()
            .all(|waypoint| waypoint.execution_contract.is_none())
    );
    assert!(
        serialized["waypoints"]
            .as_array()
            .unwrap()
            .iter()
            .all(|waypoint| waypoint.get("execution_contract").is_none())
    );
    assert!(
        legacy
            .waypoints
            .iter()
            .all(|waypoint| waypoint.outcome_plan.is_some())
    );

    let mut missing_new_field: Value = serde_json::from_slice(&bytes).unwrap();
    for waypoint in missing_new_field["waypoints"].as_array_mut().unwrap() {
        waypoint
            .as_object_mut()
            .unwrap()
            .remove("execution_contract");
    }
    let round_trip: TypedDraft = serde_json::from_value(missing_new_field).unwrap();
    assert_eq!(round_trip, legacy);
}
