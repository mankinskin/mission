#[path = "support/direct_transports.rs"]
mod direct_transports;
#[path = "support/native_guidance.rs"]
mod native_guidance;

use direct_transports::Mcp;
use native_guidance::{
    ScratchDirectory, assert_routing_contract, installation_roundtrip, source_root,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const NATIVE_TOOLS: [&str; 22] = [
    "mission_intake_create",
    "mission_workspace_get",
    "mission_plan_get",
    "mission_plan_update",
    "mission_waypoint_create",
    "mission_waypoint_update",
    "mission_waypoint_remove",
    "mission_waypoint_reorder",
    "mission_waypoint_complete",
    "mission_waypoint_activate",
    "mission_plan_prepare_review",
    "mission_plan_approve",
    "mission_plan_replan",
    "mission_pause",
    "mission_resume",
    "mission_handoff",
    "mission_cancel",
    "mission_complete",
    "mission_review_results_add_waypoints",
    "mission_reopen_most_recent_completed",
    "mission_plan_record_review",
    "mission_intent",
];

#[test]
fn native_guidance_installs_rewritten_closure_in_consumer_root_idempotently() {
    installation_roundtrip(false, false);
}

#[test]
fn native_guidance_prefers_initialized_product_submodule_without_changing_classic_sources() {
    installation_roundtrip(true, false);
}

#[test]
fn native_guidance_prefers_initialized_nested_guidance_owners() {
    installation_roundtrip(true, true);
}

#[test]
fn canonical_sources_and_existing_aliases_follow_same_native_contract() {
    assert_routing_contract(|path| fs::read_to_string(source_root().join(path)).unwrap());
    let readme =
        fs::read_to_string(source_root().join("workflow-tools/install/README.md")).unwrap();
    assert!(readme.contains("mission_plan_record_review"));
    assert!(readme.contains("simulate a real requester receipt"));
    for destination in [
        "prompt-ingestion.instructions.md#native-new-run-planning",
        "roadmap-execution.instructions.md#native-evidence-and-failure-boundary",
    ] {
        assert!(readme.contains(&format!(
            "https://github.com/mankinskin/workflow-tools/blob/main/.agents/instructions/workflow/{destination}"
        )));
    }
    #[cfg(unix)]
    for owner in ["", "workflow-tools"] {
        assert_eq!(
            fs::read_link(source_root().join(owner).join(".claude/commands")).unwrap(),
            Path::new("../.agents/prompts")
        );
    }
    #[cfg(unix)]
    for source in &native_guidance::SOURCES[..3] {
        let name = Path::new(source).file_name().unwrap();
        assert_eq!(
            fs::read(source_root().join(".claude/commands").join(name)).unwrap(),
            fs::read(source_root().join(source)).unwrap()
        );
    }
}

fn route(arguments: &[&str]) -> Output {
    Command::new("python3")
        .arg(source_root().join("workflow-tools/tools/roadmap-route.py"))
        .args(arguments)
        .output()
        .unwrap()
}

fn mcp_error_envelope(mcp: &mut Mcp, request_id: u64, tool: &str, arguments: Value) -> Value {
    let response = mcp.request(
        request_id,
        "tools/call",
        json!({"name":tool, "arguments":arguments}),
    );
    assert_eq!(response["isError"], true, "{response}");
    serde_json::from_str(response["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn classic_and_explicit_migration_routes_preserve_their_existing_output() {
    for (mode, source, action) in [
        ("classic", "dossier", "execute-dossier"),
        ("mission-backed", "mission", "migrate-dossier"),
    ] {
        let output = route(&["--mode", mode, "--dossier", "classic/ROADMAP.md"]);
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"mode":mode,"source":source,"action":action,"dossier":"classic/ROADMAP.md"})
        );
    }
}

#[test]
fn native_new_run_without_discovery_fails_visibly_without_classic_fallback() {
    let output = route(&["--mode", "native-new-run"]);
    assert!(!output.status.success());
    let error: Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)));
    assert_eq!(error["code"], "native_capability_unavailable");
    assert_eq!(error["fallback"], Value::Null);
}

#[test]
fn native_discovery_and_fresh_intake_stop_before_requester_approval() {
    use session_api::{SessionRuntimeInitRequest, SessionStoreConfig};
    let temporary = ScratchDirectory::new();
    let root = temporary.path().join("workspace");
    fs::create_dir(&root).unwrap();
    SessionStoreConfig::new(root.join(".workflow-tools/session"))
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(SESSION.into()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    let mut mcp = Mcp::start(&root);
    mcp.initialize();
    let listed = mcp.request(2, "tools/list", json!({}));
    let tools = listed["tools"].as_array().unwrap();
    for name in NATIVE_TOOLS {
        assert_eq!(
            tools.iter().filter(|tool| tool["name"] == name).count(),
            1,
            "tools/list must expose exactly one {name}"
        );
    }
    let discovery = temporary.path().join("tools.json");
    fs::write(&discovery, serde_json::to_vec(&listed).unwrap()).unwrap();
    let arguments = [
        "--mode",
        "native-new-run",
        "--workspace-root",
        root.to_str().unwrap(),
        "--session-id",
        SESSION,
        "--tools-list",
        discovery.to_str().unwrap(),
    ];
    let selected = route(&arguments);
    let review_schema = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "mission_plan_record_review")
        .unwrap()["inputSchema"]
        .clone();
    assert!(
        selected.status.success(),
        "{}; review schema: {review_schema}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let selected: Value = serde_json::from_slice(&selected.stdout).unwrap();
    assert_eq!(selected["action"], "plan-native");
    assert_eq!(selected["source"], "session");
    assert_eq!(selected["executable"], false);
    assert!(
        !root.join(".workflow-tools/mission").exists(),
        "selector must not create a Mission"
    );

    let mut missing_review = listed.clone();
    missing_review["tools"]
        .as_array_mut()
        .unwrap()
        .retain(|tool| tool["name"] != "mission_plan_record_review");
    fs::write(&discovery, serde_json::to_vec(&missing_review).unwrap()).unwrap();
    let old_capability = route(&arguments);
    assert!(
        !old_capability.status.success(),
        "the original tools alone must not select native planning"
    );
    let error: Value = serde_json::from_slice(&old_capability.stderr).unwrap();
    assert_eq!(error["code"], "native_capability_unavailable");
    assert_eq!(error["fallback"], Value::Null);
    fs::write(&discovery, serde_json::to_vec(&listed).unwrap()).unwrap();

    let intake = mcp.call(
        3,
        "mission_intake_create",
        json!({
            "workspace_root":root,"session_id":SESSION,"request_id":"w7-source",
            "input":"Keep immutable native source\n",
        }),
    );
    assert_eq!(intake["code"], "ok", "{intake}");
    let workspace = &intake["workspace"];
    assert_eq!(
        fs::read_to_string(workspace["input_path"].as_str().unwrap()).unwrap(),
        "Keep immutable native source\n"
    );
    assert!(!root.join("transcripts").exists());
    let mission_dir = Path::new(workspace["mission_dir"].as_str().unwrap());
    let source_path = Path::new(workspace["source_path"].as_str().unwrap());
    assert!(mission_dir.is_dir());
    assert!(source_path.is_file());
    assert_eq!(source_path.file_name().unwrap(), "source.json");
    let located = mcp.call(
        4,
        "mission_workspace_get",
        json!({
            "workspace_root":root,"session_id":SESSION,"request_id":"w7-source",
        }),
    );
    assert_eq!(located["workspace"], intake["workspace"]);
    assert_eq!(intake["result"]["executable"], false);
    let id = intake["mission_id"].as_str().unwrap();
    let waypoint = mission::TypedWaypoint {
        waypoint_id: "w1".into(),
        title: "Native route".into(),
        prompt: "Verify native routing".into(),
        part: mission::PartSource {
            waypoint_id: "w1".into(),
            document: mission::Document {
                blocks: vec![mission::Block::Paragraph {
                    inlines: vec![mission::Inline::Text {
                        text: "Native routing outcome".into(),
                    }],
                }],
            },
        },
        validation: vec![mission::ValidationGate {
            command: "true".into(),
            cwd: None,
        }],
        commit_checkpoint: mission::CommitCheckpoint {
            message: "Native checkpoint".into(),
        },
        non_goals: vec!["Unrelated work".into()],
        ..Default::default()
    };
    let created = mcp.call(
        5,
        "mission_waypoint_create",
        json!({
            "workspace_root":root,"mission_id":id,"request_id":"w7-waypoint",
            "expected_revision":intake["revision"],"waypoint":waypoint
        }),
    );
    assert_eq!(created["code"], "ok", "{created}");
    let read = mcp.call(
        6,
        "mission_plan_get",
        json!({"workspace_root":root,"mission_id":id}),
    );
    assert_eq!(read["result"]["grants"], json!([]));
    let review_args = json!({
        "workspace_root":root,"mission_id":id,"expected_revision":created["revision"],
        "plan_digest":read["result"]["plan"]["plan_digest"],
        "evidence_identity":read["result"]["plan"]["evidence_identity"],
        "kind":"cold_walkthrough","verdict":"pass","reviewer":"cold-reviewer",
        "request_id":"w7-cold-review"
    });
    let cold = mcp.call(7, "mission_plan_record_review", review_args.clone());
    assert_eq!(cold["code"], "ok", "{cold}");
    let mut formal_args = review_args.clone();
    formal_args["kind"] = json!("formal");
    formal_args["reviewer"] = json!("formal-reviewer");
    formal_args["request_id"] = json!("w7-formal-review");
    formal_args["cold_review_id"] = cold["result"]["review_id"].clone();
    let formal = mcp.call(8, "mission_plan_record_review", formal_args);
    assert_eq!(formal["code"], "ok", "{formal}");
    assert_eq!(cold["revision"], created["revision"]);
    assert_eq!(formal["revision"], created["revision"]);
    assert_ne!(cold["result"]["reviewer"], formal["result"]["reviewer"]);
    assert_eq!(
        formal["result"]["cold_review_id"],
        cold["result"]["review_id"]
    );
    let reviewed = mcp.call(
        9,
        "mission_plan_get",
        json!({"workspace_root":root,"mission_id":id}),
    );
    assert_eq!(
        reviewed["result"]["reviews"],
        json!([cold["result"], formal["result"]])
    );
    assert_eq!(reviewed["result"]["grants"], json!([]));
    assert_eq!(reviewed["result"]["decision_refs"], json!([]));
    let challenge = mcp_error_envelope(
        &mut mcp,
        10,
        "mission_plan_prepare_review",
        json!({
            "workspace_root":root,"mission_id":id,"request_id":"w7-review",
            "expected_revision":created["revision"],"session_id":SESSION,
        }),
    );
    assert_eq!(
        challenge["code"], "approval_policy_upgrade_required",
        "{challenge}"
    );
    let denied = mcp_error_envelope(
        &mut mcp,
        11,
        "mission_plan_approve",
        json!({
            "workspace_root":root,"mission_id":id,"request_id":"w8-no-authority",
            "expected_revision":created["revision"],
            "decision":{"session_id":SESSION,"challenge_id":"w8-no-authority",
                "decision_id":"absent","source_event_id":"absent"}
        }),
    );
    assert_eq!(denied["code"], "approval_authority_unavailable", "{denied}");
    let fake_authority = mcp_error_envelope(
        &mut mcp,
        12,
        "mission_plan_approve",
        json!({
            "workspace_root":root,"mission_id":id,"request_id":"w8-no-authority",
            "expected_revision":created["revision"],
            "decision":{"session_id":SESSION,"challenge_id":"w8-no-authority",
                "decision_id":"agent-approval","source_event_id":"invented","actor":"agent","approved":true}
        }),
    );
    assert_ne!(fake_authority["code"], "ok", "{fake_authority}");
    let after = mcp.call(
        13,
        "mission_plan_get",
        json!({"workspace_root":root,"mission_id":id}),
    );
    assert_eq!(after["result"]["grants"], json!([]));
    for mutate in [
        "missing",
        "missing_activation",
        "missing_intent",
        "version",
        "generic",
        "schema",
        "review_schema",
        "review_required",
        "review_optional",
        "review_kind",
        "review_ref",
        "review_unknown",
        "coverage_type",
        "terminal_type",
        "scope_type",
        "missing_scope",
        "malformed",
    ] {
        let mut changed = listed.clone();
        let tools = changed["tools"].as_array_mut().unwrap();
        match mutate {
            "missing" => {
                tools.retain(|tool| tool["name"] != "mission_intake_create");
            }
            "missing_activation" => {
                tools.retain(|tool| tool["name"] != "mission_waypoint_activate");
            }
            "missing_intent" => {
                tools.retain(|tool| tool["name"] != "mission_intent");
            }
            "generic" => {
                tools.retain(|tool| tool["name"] == "mission_operation");
            }
            "version" => {
                tools
                    .iter_mut()
                    .find(|tool| tool["name"] == "mission_intake_create")
                    .unwrap()["description"] =
                    json!("capability=mission.native-planning version=2");
            }
            "schema" => {
                tools
                    .iter_mut()
                    .find(|tool| tool["name"] == "mission_intake_create")
                    .unwrap()["inputSchema"] = json!({});
            }
            "malformed" => {
                changed = json!([]);
            }
            _ => {
                let schema = &mut tools
                    .iter_mut()
                    .find(|tool| tool["name"] == "mission_plan_record_review")
                    .unwrap()["inputSchema"];
                match mutate {
                    "review_schema" => schema["additionalProperties"] = json!(true),
                    "review_required" => schema["required"]
                        .as_array_mut()
                        .unwrap()
                        .retain(|field| field != "reviewer"),
                    "review_optional" => schema["required"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!("session_root")),
                    "review_kind" => {
                        schema["$defs"]["ReviewKind"]["enum"] = json!(["cold_walkthrough"])
                    }
                    "review_ref" => schema["properties"]["kind"]["enum"] = json!(["formal"]),
                    "review_unknown" => schema["properties"]["unknown"] = json!({"type":"string"}),
                    "coverage_type" => {
                        schema["properties"]["goal_coverage_findings"] = json!({"type":"string"})
                    }
                    "terminal_type" => {
                        schema["properties"]["terminal_outcome_finding"] = json!({"type":"string"})
                    }
                    "scope_type" => {
                        schema["properties"]["scope_findings"] = json!({"type":"string"})
                    }
                    "missing_scope" => {
                        schema["properties"]
                            .as_object_mut()
                            .unwrap()
                            .remove("scope_findings");
                    }
                    _ => unreachable!(),
                }
            }
        }
        fs::write(&discovery, serde_json::to_vec(&changed).unwrap()).unwrap();
        let output = route(&arguments);
        assert!(
            !output.status.success(),
            "{mutate} discovery must fail closed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(
            error["code"], "native_capability_unavailable",
            "{mutate}: {error}"
        );
        assert_eq!(error["fallback"], Value::Null);
    }
}
