use super::*;

#[test]
fn mcp_create_and_lifecycle_round_trip_through_cli_adapters() {
    let workspace = TempDir::new().expect("workspace");
    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let tools = mcp.request(20, "tools/list", json!({}));
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
    let tool_name = tool["name"].as_str().expect("tool name").to_owned();
    let created = mcp.call(21, &tool_name, json!({         "operation": "create_mission", "title": "MCP lifecycle", "objective": "Exercise adapters.",         "waypoints": [waypoint()]
    }));
    assert_ok(&created);
    assert_ok(&mcp.call(
        99,
        &tool_name,
        json!({"operation":"get_mission", "mission_id":created["mission_id"]}),
    ));
    let mission_id = created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned();
    let added = cli(
        workspace.path(),
        &[
            "add-waypoint".into(),
            mission_id.clone(),
            "verify".into(),
            "Verify".into(),
            revision(&created).to_string(),
            "--prompt".into(),
            "Verify generated output.".into(),
            "--validation-command".into(),
            "operator verify command".into(),
            "--validation-cwd".into(),
            "mission".into(),
            "--checkpoint".into(),
            "verify evidence".into(),
            "--depends-on".into(),
            "install".into(),
        ],
    );
    assert_ok(&added);
    let edited = cli(
        workspace.path(),
        &[
            "edit-waypoint".into(),
            mission_id.clone(),
            "verify".into(),
            "Verify edited".into(),
            revision(&added).to_string(),
            "--prompt".into(),
            "Verify generated output.".into(),
            "--validation-command".into(),
            "operator verify command".into(),
            "--validation-cwd".into(),
            "mission".into(),
            "--checkpoint".into(),
            "verify evidence".into(),
            "--depends-on".into(),
            "install".into(),
        ],
    );
    assert_ok(&edited);
    let ordered = mcp.call(22, &tool_name, json!({         "operation": "order_waypoints", "mission_id": mission_id,         "execution_order": ["verify", "install"],         "waypoint_dependencies": {"verify": [], "install": ["verify"]},         "expected_current_revision": revision(&edited)
    }));
    assert_ok(&ordered);
    let progressed = mcp.call(23, &tool_name, json!({         "operation": "record_progress", "mission_id": mission_id, "waypoint_id": "verify",         "state": "completed", "expected_current_revision": revision(&ordered)
    }));
    assert_eq!(progressed["status"], "error");
    assert_eq!(
        progressed["diagnostics"][0]["code"],
        "completion_evidence_required"
    );
    let progressed = mcp.call(
        230,
        &tool_name,
        json!({
            "operation": "record_progress", "mission_id": mission_id, "waypoint_id": "verify",
            "state": "blocked", "expected_current_revision": revision(&ordered)
        }),
    );
    assert_ok(&progressed);
    let evidenced = mcp.call(24, &tool_name, json!({         "operation": "record_validation_result", "mission_id": mission_id, "waypoint_id": "verify",         "outcome": "passed", "operator_command": "operator verify command",         "evidence_pointer": "ce://mcp/test/verify", "expected_current_revision": revision(&progressed)
    }));
    assert_ok(&evidenced);
    assert_ok(&mcp.call(
        25,
        &tool_name,
        json!({"operation":"validate_mission", "mission_id":mission_id}),
    ));
    let published = mcp.call(26, &tool_name, json!({         "operation":"publish_mission", "mission_id":mission_id,         "expected_current_revision":revision(&evidenced)
    }));
    assert_ok(&published);
    let before_invalid = cli(workspace.path(), &["get".into(), mission_id.clone()]);
    let invalid = mcp.call(27, &tool_name, json!({         "operation": "record_validation_result", "mission_id": mission_id, "waypoint_id": "verify",         "outcome": " ", "operator_command": "operator verify command", "evidence_pointer": "test://bad",         "expected_current_revision": revision(&before_invalid)
    }));
    assert_eq!(invalid["status"], "error", "{invalid}");
    assert!(
        invalid["diagnostics"].is_array(),
        "domain error must be structured: {invalid}"
    );
    let read = cli(workspace.path(), &["get".into(), mission_id.clone()]);
    assert_eq!(revision(&read), revision(&before_invalid));
    assert_eq!(
        accepted_waypoints(&read),
        accepted_waypoints(&before_invalid)
    );
    assert_eq!(
        read["accepted"]["bundle"]["manifest"]["operator_results"][0]["evidence_pointer"],
        "ce://mcp/test/verify"
    );
    assert_eq!(read["publication_state"], "current");
    assert_ok(&cli(
        workspace.path(),
        &["check-generated".into(), mission_id],
    ));
}
#[test]
fn cli_order_waypoints_atomically_updates_dependencies_and_preserves_state_on_rejection() {
    let workspace = TempDir::new().expect("workspace");
    let mut prepare = waypoint();
    prepare["waypoint_id"] = json!("prepare");
    prepare["title"] = json!("Prepare");
    let mut verify = waypoint();
    verify["waypoint_id"] = json!("verify");
    verify["title"] = json!("Verify");
    verify["depends_on"] = json!(["prepare"]);
    let created = cli(
        workspace.path(),
        &[
            "create-mission".into(),
            "Order".into(),
            "Atomically reorder.".into(),
            json!([prepare, verify]).to_string(),
        ],
    );
    assert_ok(&created);
    let mission_id = created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned();
    let original = cli(workspace.path(), &["get".into(), mission_id.clone()]);
    let original_ids = accepted_waypoints(&original);
    let reordered = cli(
        workspace.path(),
        &[
            "order-waypoints".into(),
            mission_id.clone(),
            "verify,prepare".into(),
            revision(&original).to_string(),
            "--waypoint-dependencies".into(),
            json!({"verify": [], "prepare": ["verify"]}).to_string(),
        ],
    );
    assert_ok(&reordered);
    let accepted = cli(workspace.path(), &["get".into(), mission_id.clone()]);
    assert_eq!(
        accepted["accepted"]["bundle"]["manifest"]["execution_order"],
        json!(["verify", "prepare"])
    );
    let accepted_ids = accepted_waypoints(&accepted);
    assert_eq!(
        accepted_ids.keys().collect::<Vec<_>>(),
        original_ids.keys().collect::<Vec<_>>()
    );
    assert_eq!(accepted_ids["verify"]["depends_on"], json!([]));
    assert_eq!(accepted_ids["prepare"]["depends_on"], json!(["verify"]));
    let accepted_revision = revision(&accepted);
    for (order, dependencies) in [
        (
            "verify,missing",
            json!({"verify": [], "missing": ["verify"]}),
        ),
        (
            "verify,verify",
            json!({"verify": [], "prepare": ["verify"]}),
        ),
        ("verify,prepare", json!({"unknown": []})),
    ] {
        let rejected = cli(
            workspace.path(),
            &[
                "order-waypoints".into(),
                mission_id.clone(),
                order.into(),
                accepted_revision.to_string(),
                "--waypoint-dependencies".into(),
                dependencies.to_string(),
            ],
        );
        assert_eq!(rejected["status"], "error", "{order}");
        let read = cli(workspace.path(), &["get".into(), mission_id.clone()]);
        assert_eq!(revision(&read), accepted_revision, "{order}");
        assert_eq!(
            read["accepted"]["bundle"]["manifest"]["execution_order"],
            accepted["accepted"]["bundle"]["manifest"]["execution_order"]
        );
        assert_eq!(accepted_waypoints(&read), accepted_ids, "{order}");
    }
}
#[test]
fn invalid_and_stale_cli_requests_preserve_accepted_state() {
    let workspace = TempDir::new().expect("workspace");
    let invalid = cli(
        workspace.path(),
        &[
            "create-mission".into(),
            " ".into(),
            "objective".into(),
            json!([waypoint()]).to_string(),
        ],
    );
    assert_eq!(invalid["status"], "error");
    assert!(
        !workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .exists()
    );
    let mut missing = waypoint();
    missing.as_object_mut().unwrap().remove("prompt");
    let missing_payload = json!([missing]).to_string();
    let missing = Command::new(env!("CARGO_BIN_EXE_mission"))
        .arg("--workspace")
        .arg(workspace.path())
        .args(["create-mission", "Direct", "objective", &missing_payload])
        .output()
        .expect("mission CLI starts");
    assert!(
        !missing.status.success(),
        "missing prompt must reject CLI arguments"
    );
    assert!(
        missing.stdout.is_empty(),
        "argument failures must not emit JSON stdout"
    );
    assert!(String::from_utf8_lossy(&missing.stderr).contains("prompt"));
    assert!(
        !workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .exists()
    );
    let created = cli(
        workspace.path(),
        &[
            "create-mission".into(),
            "Direct".into(),
            "objective".into(),
            json!([waypoint()]).to_string(),
        ],
    );
    let mission_id = created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned();
    let updated = cli(
        workspace.path(),
        &[
            "edit-mission-objective".into(),
            mission_id.clone(),
            "accepted".into(),
            revision(&created).to_string(),
        ],
    );
    let stale = cli(
        workspace.path(),
        &[
            "edit-mission-objective".into(),
            mission_id.clone(),
            "lost update".into(),
            revision(&created).to_string(),
        ],
    );
    assert_eq!(stale["status"], "error");
    let read = cli(workspace.path(), &["get".into(), mission_id]);
    assert_eq!(revision(&read), revision(&updated));
    assert_eq!(
        read["accepted"]["bundle"]["manifest"]["objective"],
        "accepted"
    );
}

#[test]
fn typed_drafts_have_cli_mcp_parity_for_identity_gates_and_serial_sources() {
    let workspace = TempDir::new().expect("workspace");
    let created = cli(
        workspace.path(),
        &[
            "create-draft".into(),
            "Typed transport".into(),
            "Exercise typed adapters.".into(),
        ],
    );
    assert_ok(&created);
    let mission_id = created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned();
    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let tools = mcp.request(40, "tools/list", json!({}));
    let tool = tools["tools"]
        .as_array()
        .expect("tools list")
        .iter()
        .find(|tool| {
            tool["description"]
                .as_str()
                .is_some_and(|text| text.contains("create_draft"))
        })
        .expect("typed MCP tool");
    assert!(tool["inputSchema"]["properties"].get("draft").is_some());
    let tool_name = tool["name"].as_str().expect("tool name").to_owned();
    let source = mcp.call(
        41,
        &tool_name,
        json!({"operation":"get_mission_source", "mission_id":mission_id}),
    );
    assert_eq!(source["source"]["mission_id"], mission_id);
    let mut incomplete = source["source"].clone();
    incomplete["waypoints"] = json!([{
        "waypoint_id": "save",
        "title": "Save",
        "prompt": "Keep this distinct from the typed Part.",
        "part": {"waypoint_id": "save", "document": {"blocks": []}},
        "validation": [],
        "commit_checkpoint": {"message": ""}
    }]);
    let saved = mcp.call(
        42,
        &tool_name,
        json!({
            "operation":"save_draft", "mission_id":mission_id, "draft":incomplete,
            "expected_current_revision":revision(&source)
        }),
    );
    assert_ok(&saved);
    let cli_source = cli(
        workspace.path(),
        &["get-mission-source".into(), mission_id.clone()],
    );
    assert_eq!(cli_source["source"], saved["source"]);
    let mcp_rejected = mcp.call(
        43,
        &tool_name,
        json!({
            "operation":"publish_draft", "mission_id":mission_id,
            "draft":saved["source"], "expected_current_revision":revision(&saved)
        }),
    );
    let cli_rejected = cli(
        workspace.path(),
        &[
            "publish-draft".into(),
            mission_id.clone(),
            saved["source"].to_string(),
            revision(&saved).to_string(),
        ],
    );
    assert_eq!(mcp_rejected["status"], "error");
    assert_eq!(mcp_rejected["diagnostics"], cli_rejected["diagnostics"]);
    assert_eq!(
        cli(
            workspace.path(),
            &["get-mission-source".into(), mission_id.clone()]
        )["revision"],
        saved["revision"]
    );
    let mut complete = saved["source"].clone();
    complete["waypoints"][0]["validation"] = json!([{"command":"cargo test"}]);
    complete["waypoints"][0]["commit_checkpoint"] = json!({"message":"typed evidence"});
    let published = cli(
        workspace.path(),
        &[
            "publish-draft".into(),
            mission_id.clone(),
            complete.to_string(),
            revision(&saved).to_string(),
        ],
    );
    assert_ok(&published);
    assert_eq!(published["source"]["waypoints"][0]["depends_on"], json!([]));
    assert_ok(&mcp.call(
        44,
        &tool_name,
        json!({"operation":"render_mission", "mission_id":mission_id}),
    ));
}
