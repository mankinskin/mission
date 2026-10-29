#[path = "support/direct_transports.rs"]
mod support;
use support::*;

#[test]
fn cli_and_live_mcp_preserve_typed_v3_plans_and_reject_missing_plans_identically() {
    let workspace = tempfile::Builder::new()
        .prefix(".w7-transport-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let created = cli(workspace.path(), &["create-draft".into(), "Planned".into(), "Objective".into()]);
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
    let published = cli(workspace.path(), &["publish-draft".into(), id.into(), draft.to_string(), "1".into()]);
    assert_ok(&published);
    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let tools = mcp.request(2, "tools/list", json!({}));
    let tool = tools["tools"].as_array().unwrap().iter().find(|tool| {
        tool["inputSchema"]["properties"].get("operation").is_some()
    }).unwrap();
    assert!(tool["inputSchema"].to_string().contains("outcome_plan"));
    let name = tool["name"].as_str().unwrap();
    let source = mcp.call(3, name, json!({"operation":"get_mission_source", "mission_id":id}));
    assert_ok(&source);
    assert_eq!(source["source"], published["source"]);
    assert_eq!(source["source"]["waypoints"][0]["outcome_plan"], plan);
    let mut invalid = source["source"].clone();
    invalid["waypoints"][0].as_object_mut().unwrap().remove("outcome_plan");
    let rejected_cli = cli(workspace.path(), &["publish-draft".into(), id.into(), invalid.to_string(), "2".into()]);
    let rejected_mcp = mcp.call(4, name, json!({
        "operation":"publish_draft", "mission_id":id,
        "draft":invalid, "expected_current_revision":2
    }));
    assert_eq!(rejected_cli["status"], "error");
    assert_eq!(rejected_cli["diagnostics"], rejected_mcp["diagnostics"]);
    let after = cli(workspace.path(), &["get-mission-source".into(), id.into()]);
    assert_eq!(after["source"], source["source"]);
}

#[test]
fn output_contract_and_separate_reaction_round_trip_between_cli_and_mcp() {
    let workspace = TempDir::new().unwrap();
    let created = cli(workspace.path(), &[
        "create-mission".into(), "History transport".into(), "Preserve states".into(),
        json!([waypoint()]).to_string(),
    ]);
    assert_ok(&created);
    let id = created["mission_id"].as_str().unwrap().to_owned();
    let created = cli(workspace.path(), &[
        "publish-mission".into(), id.clone(), revision(&created).to_string(),
    ]);
    assert_ok(&created);
    let contract = cli(workspace.path(), &["get-output-contract".into(), id.clone()]);
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
    let event = mcp.call(2, "mission_operation", json!({
        "operation": "record-approval-event", "mission_id": id,
        "approval_event_json": request.to_string()
    }));
    assert_ok(&event);
    let read = cli(workspace.path(), &["get-approval-event".into(), id.clone(), revision(&created).to_string()]);
    assert_ok(&read);
    assert_eq!(event["output_record"], read["output_record"]);
    assert_eq!(read["output_record"]["record"]["decision"], "changes-requested");
    let replay = mcp.call(3, "mission_operation", json!({
        "operation": "record-approval-event", "mission_id": id,
        "approval_event_json": request.to_string()
    }));
    assert_ne!(replay["status"], "ok");
    let checked = cli(workspace.path(), &["check-generated".into(), id.clone()]);
    assert_ok(&checked);
    assert_eq!(revision(&checked), revision(&created));
    let mcp_contract = mcp.call(4, "mission_operation", json!({
        "operation": "get-output-contract", "mission_id": id
    }));
    assert_eq!(mcp_contract["output_record"], contract["output_record"]);
    let store = mission::MissionStore::open(workspace.path());
    let uuid = uuid::Uuid::parse_str(&id).unwrap();
    let mut bundle = store.get(uuid).unwrap().bundle;
    bundle.manifest.revision += 1;
    bundle.manifest.waypoint_files = vec!["fragment.json".into()];
    let manifest = workspace.path().join("bundle.json");
    std::fs::write(workspace.path().join("fragment.json"), serde_json::to_vec(&bundle.waypoints[0]).unwrap()).unwrap();
    std::fs::write(&manifest, serde_json::to_vec(&bundle.manifest).unwrap()).unwrap();
    let published = cli(workspace.path(), &[
        "publish-with-inputs".into(), id.clone(), manifest.display().to_string(),
        revision(&created).to_string(),
        json!({"merged_request": "Merged request", "continuations": {"one.md": "First"}}).to_string(),
    ]);
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
    let directory = workspace.path().join(".workflow-tools/mission/missions").join(&id);
    assert_eq!(std::fs::read(directory.join("inputs/one.md")).unwrap(), b"First");
    assert_eq!(std::fs::read(directory.join("inputs/two.md")).unwrap(), b"Second");
    assert_eq!(std::fs::read(directory.join("REQUEST.md")).unwrap(), b"Updated merged request");
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
fn create_root_fixture(workspace: &std::path::Path, title: &str) -> String {
    let created = cli(
        workspace,
        &[
            "create-mission".into(),
            title.into(),
            "Workspace selector fixture.".into(),
            json!([waypoint()]).to_string(),
        ],
    );
    assert_ok(&created);
    created["mission_id"]
        .as_str()
        .expect("mission id")
        .to_owned()
}

fn cli_get_id(
    cwd: &std::path::Path,
    explicit: Option<&std::path::Path>,
    configured: Option<&std::ffi::OsStr>,
    mission_id: &str,
) -> Value {
    cli_launch(
        cwd,
        explicit,
        configured,
        &["get".into(), mission_id.into()],
    )
}

#[test]
fn workspace_cli_explicit_root_overrides_configured_and_relative_roots_use_cwd() {
    use std::ffi::OsStr;

    let directory = TempDir::new().expect("cwd");
    let cwd = directory.path();
    let explicit = cwd.join("explicit");
    let configured = cwd.join("configured");
    let cwd_id = create_root_fixture(cwd, "cwd fixture");
    let explicit_id = create_root_fixture(&explicit, "explicit fixture");
    let configured_id = create_root_fixture(&configured, "configured fixture");
    let nested = cwd.join("nested");
    let nested_id = create_root_fixture(&nested, "nested cwd fixture");

    let configured_read = cli_get_id(cwd, None, Some(configured.as_os_str()), &configured_id);
    assert_ok(&configured_read);
    let explicit_read = cli_get_id(
        cwd,
        Some(std::path::Path::new("explicit")),
        Some(configured.as_os_str()),
        &explicit_id,
    );
    assert_ok(&explicit_read);
    let explicit_with_empty_config = cli_get_id(
        cwd,
        Some(std::path::Path::new("explicit")),
        Some(OsStr::new("")),
        &explicit_id,
    );
    assert_ok(&explicit_with_empty_config);
    assert_eq!(
        cli_get_id(
            cwd,
            Some(std::path::Path::new("explicit")),
            Some(configured.as_os_str()),
            &configured_id
        )["status"],
        "error",
        "explicit relative workspace must override a conflicting configured root"
    );
    let cwd_read = cli_get_id(cwd, None, None, &cwd_id);
    assert_ok(&cwd_read);
    assert_ok(&cli_get_id(&nested, None, None, &nested_id));
    let missing_root = cwd.join("read-only-missing");
    let missing = cli_get_id(
        cwd,
        None,
        Some(std::path::Path::new("read-only-missing").as_os_str()),
        "00000000-0000-0000-0000-000000000000",
    );
    assert_eq!(missing["status"], "error");
    assert!(
        !missing_root
            .join(".workflow-tools/mission/missions")
            .exists()
    );
    let empty = cli_output(cwd, None, Some(OsStr::new("")), &["get".into(), cwd_id]);
    assert!(!empty.status.success());
    assert!(String::from_utf8_lossy(&empty.stderr).contains("MISSION_WORKSPACE must not be empty"));
}

#[test]
fn workspace_resolver_selects_portable_canonical_root_and_rejects_symlinks() {
    use mission::resolve_workspace;
    use std::{ffi::OsStr, path::Path};

    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("meta-workspace root");
    let (canonical, source) =
        resolve_workspace(None, Some(OsStr::new("workflow-tools/mission")), checkout)
            .expect("canonical relative launch root");
    assert_eq!(source, "configured");
    assert_eq!(canonical, checkout.join("workflow-tools/mission"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let directory = TempDir::new().expect("temporary root");
        let real = directory.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = directory.path().join("link");
        symlink(&real, &link).unwrap();
        let error = resolve_workspace(Some(&link), None, directory.path()).unwrap_err();
        assert!(error.contains("must not contain symlinks"));
    }
}

#[test]
fn workspace_mcp_explicit_configured_and_cwd_roots_are_selected() {
    use std::ffi::OsStr;

    let directory = TempDir::new().expect("cwd");
    let cwd = directory.path();
    let explicit = cwd.join("explicit");
    let configured = cwd.join("configured");
    let nested = cwd.join("nested");
    let explicit_id = create_root_fixture(&explicit, "MCP explicit fixture");
    let configured_id = create_root_fixture(&configured, "MCP configured fixture");
    let nested_id = create_root_fixture(&nested, "MCP cwd fixture");

    let mut explicit_mcp = Mcp::start_with(
        cwd,
        Some(std::path::Path::new("explicit")),
        Some(configured.as_os_str()),
    );
    explicit_mcp.initialize();
    let tools = explicit_mcp.request(2, "tools/list", json!({}));
    let name = tools["tools"][0]["name"].as_str().expect("tool name");
    assert_eq!(
        explicit_mcp.call(
            3,
            name,
            json!({"operation":"get", "mission_id":explicit_id})
        )["status"],
        "ok"
    );
    assert_eq!(
        explicit_mcp.call(
            4,
            name,
            json!({"operation":"get", "mission_id":configured_id})
        )["status"],
        "error"
    );

    let mut configured_mcp = Mcp::start_with(cwd, None, Some(OsStr::new("configured")));
    configured_mcp.initialize();
    let tools = configured_mcp.request(2, "tools/list", json!({}));
    let name = tools["tools"][0]["name"].as_str().expect("tool name");
    assert_eq!(
        configured_mcp.call(
            3,
            name,
            json!({"operation":"get", "mission_id":configured_id})
        )["status"],
        "ok"
    );

    let mut cwd_mcp = Mcp::start(&nested);
    cwd_mcp.initialize();
    let tools = cwd_mcp.request(2, "tools/list", json!({}));
    let name = tools["tools"][0]["name"].as_str().expect("tool name");
    assert_eq!(
        cwd_mcp.call(3, name, json!({"operation":"get", "mission_id":nested_id}))["status"],
        "ok"
    );
}

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
