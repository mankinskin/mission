use super::*;

#[test]
fn repair_same_identity_twelve_waypoints_and_publish_preserve_old_and_new_evidence() {
    let workspace = TempDir::new().expect("workspace");
    let (mission_id, starting_revision) = create_typed_fixture(workspace.path(), true);
    let mission_uuid = Uuid::parse_str(&mission_id).expect("Mission UUID");
    let (request, preview) = approved_preview_request(workspace.path(), &mission_id);
    assert_eq!(preview["repair"]["status"], "ready");

    let before = mission_payload(workspace.path(), &mission_id);
    let before_record = cli(workspace.path(), &["get".into(), mission_id.clone()]);
    assert_ok(&before_record);
    let original_results =
        before_record["accepted"]["bundle"]["manifest"]["operator_results"].clone();
    let original_results = original_results.as_array().unwrap();
    assert_eq!(original_results.len(), 3);
    for (index, result) in original_results.iter().enumerate() {
        assert_eq!(result["waypoint_id"], NATIVE_WAYPOINT_IDS[index]);
    }
    assert!(
        !workspace
            .path()
            .join(".workflow-tools/mission/.repair-journals")
            .exists()
    );
    assert!(
        !workspace
            .path()
            .join(".workflow-tools/mission/.repair-snapshots")
            .exists()
    );

    let candidate = &preview["repair"]["candidate_overlay"][&mission_id];
    let typed_waypoints = candidate["typed_source"]["waypoints"]
        .as_array()
        .expect("typed source waypoints");
    let accepted_waypoints = candidate["accepted"]["bundle"]["waypoints"]
        .as_array()
        .expect("accepted waypoints");
    assert_eq!(typed_waypoints.len(), 12);
    assert_eq!(accepted_waypoints.len(), 12);
    for (index, expected_id) in NATIVE_WAYPOINT_IDS.iter().enumerate() {
        assert_eq!(typed_waypoints[index]["waypoint_id"], *expected_id);
        assert_eq!(accepted_waypoints[index]["waypoint_id"], *expected_id);
        let expected_state = if index < 3 { "completed" } else { "pending" };
        assert_eq!(accepted_waypoints[index]["state"], expected_state);
    }
    assert_eq!(
        candidate["accepted"]["bundle"]["manifest"]["operator_results"][0]["waypoint_id"],
        "ownership-refactor"
    );

    let mut mcp = Mcp::start(workspace.path());
    mcp.initialize();
    let listed = mcp.request(2, "tools/list", json!({}));
    let tool = listed["tools"]
        .as_array()
        .expect("MCP tools")
        .iter()
        .find(|tool| {
            tool["name"] == "mission_operation"
                && tool["inputSchema"]["properties"]
                    .get("repair_request")
                    .is_some()
        })
        .expect("repair-enabled Mission operation");
    for field in ["proposal_id", "journal_id", "repair_request"] {
        assert!(
            tool["inputSchema"]["properties"].get(field).is_some(),
            "MCP schema is missing {field}"
        );
    }
    let applied = mcp.call(
        3,
        "mission_operation",
        json!({
            "operation":"repair_apply",
            "repair_request":request,
            "proposal_id":preview["repair"]["planned_journal_identity"].as_str().unwrap()
        }),
    );
    assert_ok(&applied);
    assert_eq!(applied["repair"]["status"], "applied");
    let journal_id = applied["repair"]["actual_journal_id"]
        .as_str()
        .expect("actual journal ID")
        .to_owned();

    let journal = cli(
        workspace.path(),
        &[
            "repair-journal".into(),
            "--journal-id".into(),
            journal_id.clone(),
        ],
    );
    assert_ok(&journal);
    assert_eq!(journal["repair"]["status"], "applied");
    let after_apply = mission_payload(workspace.path(), &mission_id);
    assert_ne!(
        after_apply, before,
        "repair must append the authoritative forward revision"
    );
    let resumed = cli(
        workspace.path(),
        &["repair-resume".into(), "--journal-id".into(), journal_id],
    );
    assert_ok(&resumed);
    assert_eq!(resumed["repair"]["status"], "applied");
    assert_eq!(mission_payload(workspace.path(), &mission_id), after_apply);

    let repaired = cli(
        workspace.path(),
        &["get-mission-source".into(), mission_id.clone()],
    );
    assert_ok(&repaired);
    let mut draft = repaired["source"].clone();
    assert_eq!(draft["revision"], starting_revision + 1);
    draft["waypoints"][0]["prompt"] = json!("Corrected prompt after repair.");
    let rebound = cli(
        workspace.path(),
        &[
            "publish-draft".into(),
            mission_id.clone(),
            draft.to_string(),
            (starting_revision + 1).to_string(),
        ],
    );
    assert_ok(&rebound);
    assert_eq!(revision(&rebound), starting_revision + 2);
    assert_eq!(rebound["source"]["waypoints"].as_array().unwrap().len(), 12);

    let new_evidence = cli(
        workspace.path(),
        &[
            "record-validation-result".into(),
            mission_id.clone(),
            "mission-repair".into(),
            "passed".into(),
            "post-repair validation".into(),
            "ce://repair/test/new".into(),
            revision(&rebound).to_string(),
        ],
    );
    assert_ok(&new_evidence);
    let completed = cli(
        workspace.path(),
        &[
            "record-progress".into(),
            mission_id.clone(),
            "mission-repair".into(),
            "completed".into(),
            revision(&new_evidence).to_string(),
        ],
    );
    assert_ok(&completed);
    let final_record = cli(workspace.path(), &["get".into(), mission_id]);
    assert_ok(&final_record);
    assert_eq!(
        final_record["accepted"]["bundle"]["waypoints"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    let final_waypoints = final_record["accepted"]["bundle"]["waypoints"]
        .as_array()
        .unwrap();
    for (index, expected_id) in NATIVE_WAYPOINT_IDS.iter().enumerate() {
        assert_eq!(final_waypoints[index]["waypoint_id"], *expected_id);
        let expected_state = if index < 3 || index == 5 {
            "completed"
        } else {
            "pending"
        };
        assert_eq!(final_waypoints[index]["state"], expected_state);
    }
    let final_results = final_record["accepted"]["bundle"]["manifest"]["operator_results"]
        .as_array()
        .unwrap();
    assert_eq!(&final_results[..original_results.len()], original_results);
    assert_eq!(final_results.len(), original_results.len() + 1);
    assert_eq!(
        final_results.last().unwrap()["waypoint_id"],
        "mission-repair"
    );
    let source = cli(
        workspace.path(),
        &["get-mission-source".into(), mission_uuid.to_string()],
    );
    assert_ok(&source);
    assert_eq!(
        source["source"]["waypoints"][0]["prompt"],
        "Corrected prompt after repair."
    );
}

#[test]
fn repair_preview_is_read_only_and_noop_apply_creates_no_journal() {
    let workspace = TempDir::new().expect("workspace");
    let (mission_id, revision_before) = create_typed_fixture(workspace.path(), false);
    let before = mission_payload(workspace.path(), &mission_id);
    let (request, preview) = approved_preview_request(workspace.path(), &mission_id);
    assert_eq!(preview["repair"]["status"], "already_conformant");
    assert_eq!(preview["repair"]["records"][0]["readiness"], "ready");
    assert_eq!(mission_payload(workspace.path(), &mission_id), before);

    let applied = run_repair_cli(
        workspace.path(),
        "repair-apply",
        &repair_cli_args(
            &request,
            preview["repair"]["planned_journal_identity"].as_str(),
        ),
    );
    assert_eq!(applied["status"], "ok", "{applied}");
    assert_eq!(applied["repair"]["status"], "already_conformant");
    assert!(applied["repair"]["actual_journal_id"].is_null());
    assert!(
        !workspace
            .path()
            .join(".workflow-tools/mission/.repair-journals")
            .exists()
    );
    assert_eq!(
        revision(&cli(workspace.path(), &["get".into(), mission_id.clone()])),
        revision_before
    );
    assert_eq!(mission_payload(workspace.path(), &mission_id), before);
}

#[test]
fn repair_apply_rejects_stale_part_fingerprint_without_additional_writes() {
    for changed_path in [
        "parts/ownership-refactor.json",
        "history/revision",
        "generated/ROADMAP.md",
    ] {
        let workspace = TempDir::new().expect("workspace");
        let (mission_id, current_revision) = create_typed_fixture(workspace.path(), true);
        let (request, preview) = approved_preview_request(workspace.path(), &mission_id);
        let relative_path = if changed_path == "history/revision" {
            format!("{changed_path}-{current_revision}.json")
        } else {
            changed_path.into()
        };
        let path = workspace
            .path()
            .join(".workflow-tools/mission/missions")
            .join(&mission_id)
            .join(relative_path);
        let mut bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("mutable Mission payload file {}: {error}", path.display())
        });
        if changed_path == "parts/ownership-refactor.json" {
            let mut part: Value = serde_json::from_slice(&bytes).expect("typed Part JSON");
            part["document"]["blocks"][0]["inlines"][0]["text"] = json!("intervening Part edit");
            bytes = serde_json::to_vec_pretty(&part).expect("serialized Part edit");
        } else {
            bytes.extend_from_slice(b"\nexternal-change\n");
        }
        fs::write(path, bytes).expect("intervening payload edit");
        let after_external_edit = mission_payload(workspace.path(), &mission_id);

        let rejected = run_repair_cli(
            workspace.path(),
            "repair-apply",
            &repair_cli_args(
                &request,
                preview["repair"]["planned_journal_identity"].as_str(),
            ),
        );
        assert_eq!(rejected["status"], "error", "{changed_path}: {rejected}");
        assert_eq!(rejected["repair"]["status"], "blocked");
        let blockers = rejected["repair"]["records"][0]["blockers"]
            .as_array()
            .expect("record blockers");
        assert!(
            blockers
                .iter()
                .any(|blocker| blocker == "payload-fingerprint-mismatch"),
            "{changed_path}: {blockers:?}"
        );
        assert_eq!(
            mission_payload(workspace.path(), &mission_id),
            after_external_edit
        );
        assert!(
            !workspace
                .path()
                .join(".workflow-tools/mission/.repair-journals")
                .exists()
        );
    }
}
