use super::*;

#[test]
fn cli_and_live_mcp_preserve_typed_v3_plans_and_reject_missing_plans_identically() {
    let workspace = tempfile::Builder::new()
        .prefix(".w7-transport-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let created = cli(
        workspace.path(),
        &["create-draft".into(), "Planned".into(), "Objective".into()],
    );
    assert_ok(&created);
    let id = created["mission_id"].as_str().unwrap();
    let plan = json!({"expected_outcome":"Ready <safe>", "alternative_outcomes":[
        {"outcome":"Blocked", "acceptable_handling":["Inspect manually"]}
    ]});
    let mut draft = created["source"].clone();
    draft["schema_version"] = json!(3);
    draft["waypoints"] = json!([{
        "waypoint_id":"verify", "title":"Verify", "prompt":"Check",
        "part":{"waypoint_id":"verify", "document":{"blocks":[]}},
        "validation":[{"command":"check"}], "commit_checkpoint":{"message":"checkpoint"},
        "outcome_plan":plan
    }]);
    let published = cli(
        workspace.path(),
        &[
            "publish-draft".into(),
            id.into(),
            draft.to_string(),
            "1".into(),
        ],
    );
    assert_ok(&published);
    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let tools = mcp.request(2, "tools/list", json!({}));
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["inputSchema"]["properties"].get("operation").is_some())
        .unwrap();
    assert!(tool["inputSchema"].to_string().contains("outcome_plan"));
    let name = tool["name"].as_str().unwrap();
    let source = mcp.call(
        3,
        name,
        json!({"operation":"get_mission_source", "mission_id":id}),
    );
    assert_ok(&source);
    assert_eq!(source["source"], published["source"]);
    assert_eq!(source["source"]["waypoints"][0]["outcome_plan"], plan);
    let mut invalid = source["source"].clone();
    invalid["waypoints"][0]
        .as_object_mut()
        .unwrap()
        .remove("outcome_plan");
    let rejected_cli = cli(
        workspace.path(),
        &[
            "publish-draft".into(),
            id.into(),
            invalid.to_string(),
            "2".into(),
        ],
    );
    let rejected_mcp = mcp.call(
        4,
        name,
        json!({
            "operation":"publish_draft", "mission_id":id,
            "draft":invalid, "expected_current_revision":2
        }),
    );
    assert_eq!(rejected_cli["status"], "error");
    assert_eq!(rejected_cli["diagnostics"], rejected_mcp["diagnostics"]);
    let after = cli(workspace.path(), &["get-mission-source".into(), id.into()]);
    assert_eq!(after["source"], source["source"]);
}

#[test]
fn output_contract_and_separate_reaction_round_trip_between_cli_and_mcp() {
    let workspace = TempDir::new().unwrap();
    let created = cli(
        workspace.path(),
        &[
            "create-mission".into(),
            "History transport".into(),
            "Preserve states".into(),
            json!([waypoint()]).to_string(),
        ],
    );
    assert_ok(&created);
    let id = created["mission_id"].as_str().unwrap().to_owned();
    let created = cli(
        workspace.path(),
        &[
            "publish-mission".into(),
            id.clone(),
            revision(&created).to_string(),
        ],
    );
    assert_ok(&created);
    let contract = cli(
        workspace.path(),
        &["get-output-contract".into(), id.clone()],
    );
    assert_ok(&contract);
    assert_eq!(contract["output_record"]["kind"], "contract");
    let request = json!({
        "revision": revision(&created),
        "decision": "changes-requested",
        "signature": contract["output_record"]["record"]["binding"],
        "actor": "synthetic requester",
        "actor_source": "transport fixture",
        "metadata": {"channel": "review"}
    });
    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let event = mcp.call(
        2,
        "mission_operation",
        json!({
            "operation": "record-approval-event", "mission_id": id,
            "approval_event_json": request.to_string()
        }),
    );
    assert_ok(&event);
    let read = cli(
        workspace.path(),
        &[
            "get-approval-event".into(),
            id.clone(),
            revision(&created).to_string(),
        ],
    );
    assert_ok(&read);
    assert_eq!(event["output_record"], read["output_record"]);
    assert_eq!(
        read["output_record"]["record"]["decision"],
        "changes-requested"
    );
    let replay = mcp.call(
        3,
        "mission_operation",
        json!({
            "operation": "record-approval-event", "mission_id": id,
            "approval_event_json": request.to_string()
        }),
    );
    assert_ne!(replay["status"], "ok");
    let checked = cli(workspace.path(), &["check-generated".into(), id.clone()]);
    assert_ok(&checked);
    assert_eq!(revision(&checked), revision(&created));
    let mcp_contract = mcp.call(
        4,
        "mission_operation",
        json!({
            "operation": "get-output-contract", "mission_id": id
        }),
    );
    assert_eq!(mcp_contract["output_record"], contract["output_record"]);
    let store = mission::MissionStore::open(workspace.path());
    let uuid = uuid::Uuid::parse_str(&id).unwrap();
    let mut bundle = store.get(uuid).unwrap().bundle;
    bundle.manifest.revision += 1;
    bundle.manifest.waypoint_files = vec!["fragment.json".into()];
    let manifest = workspace.path().join("bundle.json");
    std::fs::write(
        workspace.path().join("fragment.json"),
        serde_json::to_vec(&bundle.waypoints[0]).unwrap(),
    )
    .unwrap();
    std::fs::write(&manifest, serde_json::to_vec(&bundle.manifest).unwrap()).unwrap();
    let published = cli(
        workspace.path(),
        &[
            "publish-with-inputs".into(),
            id.clone(),
            manifest.display().to_string(),
            revision(&created).to_string(),
            json!({"merged_request": "Merged request", "continuations": {"one.md": "First"}})
                .to_string(),
        ],
    );
    assert_ok(&published);
    bundle.manifest.revision += 1;
    std::fs::write(&manifest, serde_json::to_vec(&bundle.manifest).unwrap()).unwrap();
    let published = mcp.call(5, "mission_operation", json!({
        "operation": "publish-with-inputs", "mission_id": id,
        "manifest_path": manifest,
        "expected_current_revision": revision(&published),
        "output_inputs": {"merged_request": "Updated merged request", "continuations": {"two.md": "Second"}}
    }));
    assert_ok(&published);
    store.verify_output_history(uuid).unwrap();
    let directory = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(&id);
    assert_eq!(
        std::fs::read(directory.join("inputs/one.md")).unwrap(),
        b"First"
    );
    assert_eq!(
        std::fs::read(directory.join("inputs/two.md")).unwrap(),
        b"Second"
    );
    assert_eq!(
        std::fs::read(directory.join("REQUEST.md")).unwrap(),
        b"Updated merged request"
    );
    assert!(store.approval_event(uuid, revision(&published)).is_err());
}

#[test]
fn cli_create_mcp_edit_and_cli_publish_use_real_transports() {
    let workspace = TempDir::new().expect("workspace");
    let created = cli(
        workspace.path(),
        &[
            "create-mission".into(),
            "Direct transport".into(),
            "Prove direct authoring.".into(),
            json!([waypoint()]).to_string(),
        ],
    );
    assert_ok(&created);
    let mission_id = created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned();
    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let tools = mcp.request(2, "tools/list", json!({}));
    let tool = tools["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .find(|tool| {
            tool["description"]
                .as_str()
                .is_some_and(|text| text.contains("create_mission"))
        })
        .expect("direct-authoring MCP tool");
    let description = tool["description"].as_str().unwrap();
    for operation in [
        "create_mission",
        "edit_mission_objective",
        "add_waypoint",
        "edit_waypoint",
        "order_waypoints",
        "record_progress",
        "record_validation_result",
        "validate_mission",
        "publish_mission",
    ] {
        assert!(
            description.contains(operation),
            "description lacks {operation}"
        );
    }

    for field in ["prompt", "validation", "commit_checkpoint", "non_goals"] {
        assert!(
            tool["inputSchema"]["properties"].get(field).is_some(),
            "schema lacks {field}"
        );
    }
    let tool_name = tool["name"].as_str().expect("tool name").to_owned();
    let read = mcp.call(
        3,
        &tool_name,
        json!({"operation":"get", "mission_id":mission_id}),
    );
    assert_ok(&read);
    assert_eq!(
        read["accepted"]["bundle"]["waypoints"][0]["prompt"],
        waypoint()["prompt"]
    );
    assert_eq!(
        read["accepted"]["bundle"]["waypoints"][0]["validation"],
        waypoint()["validation"]
    );
    assert_eq!(
        read["accepted"]["bundle"]["waypoints"][0]["commit_checkpoint"],
        waypoint()["commit_checkpoint"]
    );
    assert_eq!(
        read["accepted"]["bundle"]["waypoints"][0]["non_goals"],
        waypoint()["non_goals"]
    );
    let edited = mcp.call(         4,         &tool_name,         json!({             "operation":"edit_mission_objective", "mission_id":mission_id,             "objective":"Edited through MCP.", "expected_current_revision":revision(&read)         }),
    );
    assert_ok(&edited);
    let evidence = cli(
        workspace.path(),
        &[
            "record-validation-result".into(),
            mission_id.clone(),
            "install".into(),
            "passed".into(),
            "operator test command".into(),
            "ce://direct/test/1".into(),
            revision(&edited).to_string(),
        ],
    );
    assert_ok(&evidence);
    let progress = cli(
        workspace.path(),
        &[
            "record-progress".into(),
            mission_id.clone(),
            "install".into(),
            "completed".into(),
            revision(&evidence).to_string(),
        ],
    );
    assert_eq!(progress["status"], "error");
    assert_eq!(
        progress["diagnostics"][0]["code"],
        "completion_evidence_required"
    );
    let progress = cli(
        workspace.path(),
        &[
            "record-progress".into(),
            mission_id.clone(),
            "install".into(),
            "blocked".into(),
            revision(&evidence).to_string(),
        ],
    );
    assert_ok(&progress);
    let published = cli(
        workspace.path(),
        &[
            "publish-mission".into(),
            mission_id.clone(),
            revision(&progress).to_string(),
        ],
    );
    assert_ok(&published);
    assert_ok(&cli(
        workspace.path(),
        &["check-generated".into(), mission_id],
    ));
}
