#[path = "named_mcp/approval_and_completion.rs"]
mod approval_and_completion;
#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/direct_transports.rs"]
mod direct_transports;
#[path = "support/host_hook.rs"]
mod host_hook;
#[path = "named_mcp/lifecycle.rs"]
mod lifecycle;
#[path = "named_mcp/activation.rs"]
mod activation;
#[path = "named_mcp/planning_and_errors.rs"]
mod planning_and_errors;
#[path = "named_mcp/review_recording.rs"]
mod review_recording;

use direct_transports::{Mcp, TempDir, json};
use session_api::{SessionRuntimeInitRequest, SessionStoreConfig};

const NAMED_TOOLS: [&str; 21] = [
    "mission_intake_create",
    "mission_workspace_get",
    "mission_plan_get",
    "mission_plan_update",
    "mission_waypoint_create",
    "mission_waypoint_update",
    "mission_waypoint_remove",
    "mission_waypoint_reorder",
    "mission_plan_prepare_review",
    "mission_plan_record_review",
    "mission_plan_approve",
    "mission_plan_replan",
    "mission_pause",
    "mission_resume",
    "mission_handoff",
    "mission_cancel",
    "mission_complete",
    "mission_review_results_add_waypoints",
    "mission_reopen_most_recent_completed",
    "mission_waypoint_complete",
    "mission_waypoint_activate",
];

fn fixture() -> (TempDir, std::path::PathBuf, String) {
    let dir = tempfile::Builder::new()
        .prefix("w6-named-mcp-")
        .tempdir()
        .expect("project-local test fixture");
    let workspace = dir.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace fixture");
    let session_id = "11111111-1111-4111-8111-111111111111".to_string();
    SessionStoreConfig::new(workspace.join(".workflow-tools/session"))
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(session_id.clone()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .expect("Session fixture");
    (dir, workspace, session_id)
}

fn call(mcp: &mut Mcp, id: u64, name: &str, arguments: serde_json::Value) -> serde_json::Value {
    let result = mcp.request(
        id,
        "tools/call",
        json!({"name": name, "arguments": arguments}),
    );
    let envelope: serde_json::Value = serde_json::from_str(
        result["content"][0]["text"]
            .as_str()
            .expect("tool returns structured JSON text"),
    )
    .expect("tool envelope JSON");
    assert_eq!(
        result["isError"].as_bool().unwrap_or(false),
        envelope["code"] != "ok",
        "{result}"
    );
    envelope
}

fn waypoint(id: &str) -> mission::TypedWaypoint {
    mission::TypedWaypoint {
        waypoint_id: id.into(),
        title: format!("{id} title"),
        prompt: format!("{id} prompt"),
        part: mission::PartSource {
            waypoint_id: id.into(),
            document: mission::Document {
                blocks: vec![mission::Block::Paragraph {
                    inlines: vec![mission::Inline::Text {
                        text: format!("{id} body"),
                    }],
                }],
            },
        },
        validation: vec![mission::ValidationGate {
            command: "true".into(),
            cwd: None,
        }],
        commit_checkpoint: mission::CommitCheckpoint {
            message: format!("{id} checkpoint"),
        },
        non_goals: vec!["Unrelated work".into()],
        ..Default::default()
    }
}

#[test]
fn tools_list_explains_optional_session_store_and_every_top_level_argument() {
    let (_dir, workspace, _session_id) = fixture();
    let mut mcp = Mcp::start(&workspace);
    mcp.initialize();
    let listed = mcp.request(2, "tools/list", json!({}));
    let tools = listed["tools"].as_array().unwrap();
    assert_eq!(tools.len(), NAMED_TOOLS.len() + 1);
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let schema = &tool["inputSchema"];
        for (field, property) in schema["properties"].as_object().unwrap() {
            assert!(
                property["description"]
                    .as_str()
                    .is_some_and(|text| !text.trim().is_empty()),
                "{name}.{field} has no tool-owned usage contract"
            );
        }
        if schema["properties"].get("expected_revision").is_some() {
            let description = schema["properties"]["request_id"]["description"]
                .as_str()
                .unwrap();
            assert!(description.contains("1..=128 visible ASCII"));
            assert!(description.contains("no spaces"));
        }
        if schema["properties"].get("session_root").is_some() {
            let description = schema["properties"]["session_root"]["description"]
                .as_str()
                .unwrap();
            for contract in [
                "omit",
                ".workflow-tools/session",
                "store directory",
                "repository root",
            ] {
                assert!(description.contains(contract), "{name}: missing {contract}");
            }
            assert!(
                !schema["required"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("session_root")),
                "{name}: session_root must remain optional"
            );
        }
    }
}

#[test]
fn intake_session_store_omission_and_explicit_directory_match_but_repo_root_does_not() {
    let (_dir, workspace, session_id) = fixture();
    let mut mcp = Mcp::start(&workspace);
    mcp.initialize();
    let args = json!({
        "workspace_root": workspace, "session_id": session_id,
        "request_id": "store-contract", "input": "Preserve store selection."
    });
    let omitted = call(&mut mcp, 2, "mission_intake_create", args.clone());
    assert_eq!(omitted["code"], "ok", "{omitted}");
    let mut explicit = args.clone();
    explicit["session_root"] = json!(workspace.join(".workflow-tools/session"));
    let replayed = call(&mut mcp, 3, "mission_intake_create", explicit);
    assert_eq!(replayed["code"], "ok", "{replayed}");
    assert_eq!(replayed["mission_id"], omitted["mission_id"]);
    let mut incorrect = args;
    incorrect["session_root"] = json!(workspace);
    let rejected = call(&mut mcp, 4, "mission_intake_create", incorrect);
    assert_ne!(rejected["code"], "ok", "{rejected}");
}

#[test]
fn stdio_named_tools_list_and_plan_calls_use_shared_envelopes() {
    let (_dir, workspace, session_id) = fixture();
    let mut mcp = Mcp::start(&workspace);
    mcp.initialize();

    let listed = mcp.request(2, "tools/list", json!({}));
    let tools = listed["tools"].as_array().expect("tools/list array");
    for name in NAMED_TOOLS {
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing named MCP tool {name}"));
        assert!(tool["description"].as_str().is_some_and(|description| {
            description.contains("capability=mission.native-planning")
                && description.contains("version=1")
        }));
        assert!(tool["inputSchema"]["type"] == "object");
        assert!(
            tool["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .contains(&json!("workspace_root"))
        );
        if name == "mission_waypoint_complete" {
            let schema = &tool["inputSchema"];
            for field in [
                "mission_id",
                "waypoint_id",
                "request_id",
                "expected_revision",
                "expected_control_sequence",
                "evidence",
            ] {
                assert!(
                    schema["required"]
                        .as_array()
                        .unwrap()
                        .contains(&json!(field))
                );
            }
            assert_eq!(schema["properties"]["request_id"]["format"], "uuid");
        }
    }

    let intake = call(
        &mut mcp,
        3,
        "mission_intake_create",
        json!({
            "workspace_root": workspace,
            "session_id": session_id,
            "request_id": "w6-intake",
            "input": "Create a versioned Mission plan."
        }),
    );
    assert_eq!(intake["code"], "ok", "{intake}");
    let mission_id = intake["mission_id"].as_str().expect("Mission ID");
    let initial_revision = intake["revision"].as_u64().expect("Mission revision");
    let created = call(
        &mut mcp,
        30,
        "mission_waypoint_create",
        json!({
            "workspace_root": workspace, "mission_id": mission_id,
            "request_id":"first-waypoint", "expected_revision": initial_revision,
            "waypoint": waypoint("one")
        }),
    );
    assert_eq!(created["code"], "ok", "{created}");
    let revision = created["revision"].as_u64().unwrap();

    let updated = call(
        &mut mcp,
        4,
        "mission_plan_update",
        json!({
            "workspace_root": workspace,
            "mission_id": mission_id,
            "request_id": "w6-goal",
            "expected_revision": revision,
            "operation": {
                "op": "set_goal",
                "title": "Named MCP plan",
                "objective": "Expose granular plan updates."
            }
        }),
    );
    assert_eq!(updated["code"], "ok", "{updated}");
    assert_eq!(updated["mission_id"], mission_id);
    assert_eq!(updated["revision"], revision + 1);
    assert_eq!(updated["capability"], "mission.native-planning");
    assert_eq!(updated["version"], 1);

    let stale = call(
        &mut mcp,
        5,
        "mission_plan_update",
        json!({
            "workspace_root": workspace,
            "mission_id": mission_id,
            "request_id": "w6-stale-goal",
            "expected_revision": revision,
            "operation": {
                "op": "set_goal",
                "title": "Stale",
                "objective": "Must not publish."
            }
        }),
    );
    assert_eq!(stale["code"], "revision_conflict", "{stale}");

    let legacy = mcp.request(
        6,
        "tools/call",
        json!({
            "name": "mission_operation",
            "arguments": {"operation": "get", "mission_id": mission_id}
        }),
    );
    assert_eq!(legacy["isError"], false);
}
