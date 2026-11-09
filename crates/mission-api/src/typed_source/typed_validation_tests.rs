use super::*;
use serde_json::{Value, json};

fn v4_source() -> Value {
    json!({
        "mission_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "schema_version": 4,
        "revision": 1,
        "title": "Contract fixture",
        "objective": "Verify a bounded contract.",
        "waypoints": [{
            "waypoint_id": "w1",
            "title": "One bounded task",
            "prompt": "Implement the declared outcome.",
            "part": {
                "waypoint_id": "w1",
                "document": {"blocks": []}
            },
            "work_kind": "implementation",
            "outcome_plan": {
                "expected_outcome": "A verified implementation",
                "alternative_outcomes": [{
                    "outcome": "Blocked",
                    "acceptable_handling": ["Escalate with diagnostics"]
                }]
            },
            "validation": [{"command": "cargo test -p mission-api", "cwd": "."}],
            "commit_checkpoint": {"message": "feat(mission-api): define contracts"},
            "execution_contract": {
                "schema_version": 4,
                "outcome_category": "working_behavior",
                "expected_outcome": "A verified implementation",
                "work_kind": "implementation",
                "rationale": "The declared unit is independently deliverable.",
                "authoritative_context": [{
                    "id": "context-spec",
                    "reference": "ce://default/spec/aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa#EA-01",
                    "description": "Execution contract acceptance"
                }],
                "boundary": {
                    "task": "ticket:bounded-work",
                    "paths": ["workflow-tools/mission"]
                },
                "declared_dependency_ids": [],
                "no_dependencies_reason": "This waypoint is the first independent unit.",
                "preconditions": [{
                    "id": "pre-source",
                    "description": "The accepted source is available.",
                    "check_ids": ["check-preflight"]
                }],
                "postconditions": [{
                    "id": "post-behavior",
                    "description": "The implementation passes its acceptance check.",
                    "check_ids": ["check-acceptance"]
                }],
                "checks": [
                    {
                        "id": "check-preflight",
                        "command": "cargo check -p mission-api",
                        "cwd": ".",
                        "kind": "preflight",
                        "evidence_class": "deterministic_artifact"
                    },
                    {
                        "id": "check-acceptance",
                        "command": "cargo test -p mission-api",
                        "cwd": ".",
                        "kind": "acceptance",
                        "evidence_class": "executable_behavior"
                    },
                    {
                        "id": "check-regression",
                        "command": "cargo test -p mission-api",
                        "cwd": ".",
                        "kind": "regression",
                        "evidence_class": "executable_behavior"
                    }
                ],
                "evidence_requirements": [{
                    "id": "evidence-test",
                    "class": "executable_behavior",
                    "check_ids": ["check-acceptance"]
                }],
                "regression_check_ids": ["check-regression"],
                "no_regression_checks_reason": null,
                "tool_policy": {
                    "actions": ["filesystem_read", "filesystem_write", "cli_read_only"],
                    "read_roots": ["workflow-tools"],
                    "write_roots": ["workflow-tools/mission"],
                    "permissions": ["read", "write", "execute"]
                },
                "environment": {
                    "profile": "rust-workspace",
                    "description": "Repository-selected Rust toolchain.",
                    "preflight_check_ids": ["check-preflight"]
                },
                "failure_policy": {
                    "trigger": "A required check fails.",
                    "diagnostic_action": "Inspect the failure and rerun after a justified fix.",
                    "max_attempts": 1,
                    "exhaustion": "block",
                    "escalation_target": "Ticket owner"
                },
                "handover": null,
                "no_handover_reason": "This is a terminal task with no downstream consumer.",
                "provider_route": null,
                "no_provider_reason": "No external runtime provider is required."
            }
        }]
    })
}

#[test]
fn v4_typed_source_accepts_a_complete_execution_contract() {
    let draft: TypedDraft = serde_json::from_value(v4_source())
        .expect("the versioned execution contract should deserialize");

    validate_typed(&draft, true)
        .expect("a complete independently checkable contract should validate");
}

#[test]
fn v4_typed_source_rejects_a_missing_execution_contract() {
    let mut source = v4_source();
    source["waypoints"][0]
        .as_object_mut()
        .expect("waypoint object")
        .remove("execution_contract");
    let draft: TypedDraft = serde_json::from_value(source).expect("legacy-compatible source shape");

    let error = validate_typed(&draft, true).expect_err("schema v4 requires a contract");
    assert_eq!(
        error.diagnostics().expect("validation diagnostics")[0].code,
        "execution-contract-required"
    );
}

#[test]
fn v4_contracts_reject_empty_unknown_unsafe_and_dangling_values() {
    let cases = [
        ("expected_outcome", json!("")),
        ("rationale", json!("  ")),
        ("authoritative_context", json!([])),
        (
            "boundary",
            json!({"task":"ticket:work","paths":["../outside"]}),
        ),
        (
            "tool_policy",
            json!({"actions":["filesystem_read"],"read_roots":["/"],"write_roots":[],"permissions":["read"],"no_write_reason":"No writes."}),
        ),
        ("no_handover_reason", Value::Null),
        ("no_provider_reason", Value::Null),
    ];
    for (field, value) in cases {
        let mut source = v4_source();
        source["waypoints"][0]["execution_contract"][field] = value;
        let draft: TypedDraft = serde_json::from_value(source).unwrap();
        assert!(
            validate_typed(&draft, true).is_err(),
            "invalid field {field} should fail closed"
        );
    }

    let mut unknown_action = v4_source();
    unknown_action["waypoints"][0]["execution_contract"]["tool_policy"]["actions"] =
        json!(["unrestricted_shell"]);
    assert!(serde_json::from_value::<TypedDraft>(unknown_action).is_err());

    let mut unknown_field = v4_source();
    unknown_field["waypoints"][0]["execution_contract"]["unreviewed"] = json!(true);
    assert!(serde_json::from_value::<TypedDraft>(unknown_field).is_err());

    let mut missing_field = v4_source();
    missing_field["waypoints"][0]["execution_contract"]
        .as_object_mut()
        .unwrap()
        .remove("rationale");
    assert!(serde_json::from_value::<TypedDraft>(missing_field).is_err());

    let mut unknown_check = v4_source();
    unknown_check["waypoints"][0]["execution_contract"]["postconditions"][0]["check_ids"] =
        json!(["not-declared"]);
    let draft: TypedDraft = serde_json::from_value(unknown_check).unwrap();
    assert!(validate_typed(&draft, true).is_err());

    let mut duplicate_id = v4_source();
    duplicate_id["waypoints"][0]["execution_contract"]["preconditions"][0]["id"] =
        json!("context-spec");
    let draft: TypedDraft = serde_json::from_value(duplicate_id).unwrap();
    assert!(validate_typed(&draft, true).is_err());

    let mut bad_dependency = v4_source();
    bad_dependency["waypoints"][0]["depends_on"] = json!(["missing"]);
    bad_dependency["waypoints"][0]["execution_contract"]["declared_dependency_ids"] =
        json!(["missing"]);
    bad_dependency["waypoints"][0]["execution_contract"]["no_dependencies_reason"] = Value::Null;
    let draft: TypedDraft = serde_json::from_value(bad_dependency).unwrap();
    assert!(validate_typed(&draft, true).is_err());
}

#[test]
fn v4_contract_dependency_cycles_and_unknown_handover_consumers_fail() {
    let mut source = v4_source();
    let mut second = source["waypoints"][0].clone();
    second["waypoint_id"] = json!("w2");
    second["part"]["waypoint_id"] = json!("w2");
    second["depends_on"] = json!(["w1"]);
    second["execution_contract"]["declared_dependency_ids"] = json!(["w1"]);
    second["execution_contract"]["no_dependencies_reason"] = Value::Null;
    source["waypoints"][0]["depends_on"] = json!(["w2"]);
    source["waypoints"][0]["execution_contract"]["declared_dependency_ids"] = json!(["w2"]);
    source["waypoints"][0]["execution_contract"]["no_dependencies_reason"] = Value::Null;
    source["waypoints"].as_array_mut().unwrap().push(second);
    let draft: TypedDraft = serde_json::from_value(source).unwrap();
    let error = validate_typed(&draft, true).unwrap_err();
    assert_eq!(
        error.diagnostics().unwrap()[0].code,
        "execution-dependency-cycle"
    );

    let mut bad_handover = v4_source();
    bad_handover["waypoints"][0]["execution_contract"]["handover"] = json!({
        "id": "handover-api",
        "producer": {
            "waypoint_id": "w1",
            "artifact_id": "api",
            "content_reference": "api.json",
            "schema": "v1",
            "root": "workflow-tools",
            "check_ids": ["check-acceptance"]
        },
        "consumer": {
            "waypoint_id": "missing",
            "artifact_id": "api",
            "content_reference": "api.json",
            "schema": "v1",
            "root": "workflow-tools",
            "check_ids": ["check-acceptance"]
        }
    });
    bad_handover["waypoints"][0]["execution_contract"]["no_handover_reason"] = Value::Null;
    let draft: TypedDraft = serde_json::from_value(bad_handover).unwrap();
    let error = validate_typed(&draft, true).unwrap_err();
    assert_eq!(
        error.diagnostics().unwrap()[0].code,
        "execution-handover-unknown-consumer"
    );
}

#[test]
fn v4_contract_write_roots_stay_within_boundary_paths() {
    for (boundary, write_root) in [
        ("src/small", "."),
        ("src/small", "src/smallish"),
        ("src/small", "../outside"),
    ] {
        let mut source = v4_source();
        source["waypoints"][0]["execution_contract"]["boundary"]["paths"] = json!([boundary]);
        source["waypoints"][0]["execution_contract"]["tool_policy"]["write_roots"] =
            json!([write_root]);
        let draft: TypedDraft = serde_json::from_value(source).unwrap();
        assert!(
            validate_typed(&draft, true).is_err(),
            "write root {write_root:?} must not escape boundary {boundary:?}"
        );
    }

    let mut nested = v4_source();
    nested["waypoints"][0]["execution_contract"]["boundary"]["paths"] = json!(["src/small"]);
    nested["waypoints"][0]["execution_contract"]["tool_policy"]["write_roots"] =
        json!(["src/small/unit"]);
    let draft: TypedDraft = serde_json::from_value(nested).unwrap();
    validate_typed(&draft, true).expect("a nested write root is within its task boundary");
}

#[test]
fn v4_handover_checks_must_support_consumer_preconditions_and_producer_postconditions() {
    let mut source = v4_source();
    let mut consumer = source["waypoints"][0].clone();
    consumer["waypoint_id"] = json!("w2");
    consumer["part"]["waypoint_id"] = json!("w2");
    consumer["depends_on"] = json!(["w1"]);
    consumer["execution_contract"]["declared_dependency_ids"] = json!(["w1"]);
    consumer["execution_contract"]["no_dependencies_reason"] = Value::Null;
    consumer["execution_contract"]["no_handover_reason"] = json!("Consumer contract.");
    source["waypoints"][0]["execution_contract"]["handover"] = json!({
        "id": "handover-api",
        "producer": {
            "waypoint_id": "w1",
            "artifact_id": "api",
            "content_reference": "api.json",
            "schema": "v1",
            "root": "workflow-tools",
            "check_ids": ["check-acceptance"]
        },
        "consumer": {
            "waypoint_id": "w2",
            "artifact_id": "api",
            "content_reference": "api.json",
            "schema": "v1",
            "root": "workflow-tools",
            "check_ids": ["check-regression"]
        }
    });
    source["waypoints"][0]["execution_contract"]["no_handover_reason"] = Value::Null;
    source["waypoints"].as_array_mut().unwrap().push(consumer);

    let draft: TypedDraft = serde_json::from_value(source.clone()).unwrap();
    assert!(
        validate_typed(&draft, true).is_err(),
        "consumer handover checks detached from preconditions must fail"
    );

    source["waypoints"][0]["execution_contract"]["handover"]["consumer"]["check_ids"] =
        json!(["check-preflight"]);
    let linked_consumer: TypedDraft = serde_json::from_value(source.clone()).unwrap();
    validate_typed(&linked_consumer, true)
        .expect("handover checks linked to consumer preconditions pass");

    source["waypoints"][0]["execution_contract"]["handover"]["producer"]["check_ids"] =
        json!(["check-regression"]);
    let detached_producer: TypedDraft = serde_json::from_value(source).unwrap();
    assert!(
        validate_typed(&detached_producer, true).is_err(),
        "producer handover checks detached from postconditions must fail"
    );
}

#[test]
fn v3_sources_remain_contract_free_and_valid() {
    let mut source = v4_source();
    source["schema_version"] = json!(3);
    source["waypoints"][0]
        .as_object_mut()
        .unwrap()
        .remove("execution_contract");
    let draft: TypedDraft = serde_json::from_value(source).unwrap();

    validate_typed(&draft, true).expect("v3 remains a readable planned-outcome schema");
    let serialized = serde_json::to_value(draft).unwrap();
    assert!(
        serialized["waypoints"][0]
            .get("execution_contract")
            .is_none()
    );
}
