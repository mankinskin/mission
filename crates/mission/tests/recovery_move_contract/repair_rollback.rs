use super::*;

#[test]
fn repair_preview_rejects_a_mismatched_explicit_workspace_root() {
    let workspace = TempDir::new().expect("workspace");
    let (mission_id, current_revision) = create_typed_fixture(workspace.path(), true);
    let wrong_root = workspace.path().join("other-store");
    fs::create_dir(&wrong_root).expect("other store");
    let request = json!({
        "source_root": wrong_root,
        "policy": REPAIR_POLICY,
        "records": [{
            "mission_id": mission_id,
            "expected_revision": current_revision,
            "accepted_sha256": "",
            "source_sha256": "",
            "payload_fingerprint": ""
        }]
    });
    let rejected = run_repair_cli(
        workspace.path(),
        "repair-preview",
        &repair_cli_args(&request, None),
    );
    assert_eq!(rejected["status"], "error");
    assert!(
        rejected["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("request source_root must match")
    );

    let rejected_apply = run_repair_cli(
        workspace.path(),
        "repair-apply",
        &repair_cli_args(&request, Some("sha256:not-a-proposal")),
    );
    assert_eq!(rejected_apply["status"], "error");
    assert!(
        rejected_apply["diagnostics"][0]["message"]
            .as_str()
            .unwrap()
            .contains("request source_root must match")
    );
}

#[test]
fn repair_rollback_appends_compensation_and_preserves_prior_history() {
    let workspace = TempDir::new().expect("workspace");
    let (mission_id, original_revision) = create_typed_fixture(workspace.path(), true);
    let history_path = workspace.path().join(format!(
        ".workflow-tools/mission/missions/{mission_id}/history/revision-{original_revision}.json"
    ));
    let old_history = fs::read(&history_path).expect("original history");
    let (request, preview) = approved_preview_request(workspace.path(), &mission_id);
    let applied = run_repair_cli(
        workspace.path(),
        "repair-apply",
        &repair_cli_args(
            &request,
            preview["repair"]["planned_journal_identity"].as_str(),
        ),
    );
    assert_ok(&applied);
    let journal_id = applied["repair"]["actual_journal_id"]
        .as_str()
        .expect("journal ID")
        .to_owned();

    let rolled_back = cli(
        workspace.path(),
        &[
            "repair-rollback".into(),
            "--journal-id".into(),
            journal_id.clone(),
        ],
    );
    assert_ok(&rolled_back);
    assert_eq!(rolled_back["repair"]["status"], "rolled-back");
    assert_eq!(
        fs::read(history_path).expect("preserved original history"),
        old_history
    );
    let restored = cli(
        workspace.path(),
        &["get-mission-source".into(), mission_id.clone()],
    );
    assert_ok(&restored);
    assert_eq!(restored["source"]["revision"], original_revision + 2);
    assert_eq!(
        restored["source"]["waypoints"].as_array().unwrap().len(),
        12
    );
    let repeated = cli(
        workspace.path(),
        &["repair-rollback".into(), "--journal-id".into(), journal_id],
    );
    assert_ok(&repeated);
    assert_eq!(repeated["repair"]["status"], "rolled-back");
}

#[test]
fn repair_rollback_recognizes_durable_compensation_after_interruption() {
    let workspace = TempDir::new().expect("workspace");
    let (mission_id, _) = create_typed_fixture(workspace.path(), true);
    let (request, preview) = approved_preview_request(workspace.path(), &mission_id);
    let applied = run_repair_cli(
        workspace.path(),
        "repair-apply",
        &repair_cli_args(
            &request,
            preview["repair"]["planned_journal_identity"].as_str(),
        ),
    );
    assert_ok(&applied);
    let journal_id = applied["repair"]["actual_journal_id"]
        .as_str()
        .expect("journal ID")
        .to_owned();

    let rolled_back = cli(
        workspace.path(),
        &[
            "repair-rollback".into(),
            "--journal-id".into(),
            journal_id.clone(),
        ],
    );
    assert_ok(&rolled_back);
    assert_eq!(rolled_back["repair"]["status"], "rolled-back");
    let compensated_payload = mission_payload(workspace.path(), &mission_id);
    let compensated_revision =
        revision(&cli(workspace.path(), &["get".into(), mission_id.clone()]));

    let journal_path = workspace
        .path()
        .join(".workflow-tools/mission/.repair-journals")
        .join(format!("{journal_id}.json"));
    let mut journal: Value =
        serde_json::from_slice(&fs::read(&journal_path).expect("read repair journal"))
            .expect("valid repair journal");
    journal["state"] = json!("partial");
    journal["records"][0]["state"] = json!("rolling-back");
    fs::write(
        &journal_path,
        serde_json::to_vec_pretty(&journal).expect("serialize interrupted journal"),
    )
    .expect("write interrupted journal state");

    let resumed = cli(
        workspace.path(),
        &["repair-rollback".into(), "--journal-id".into(), journal_id],
    );
    assert_ok(&resumed);
    assert_eq!(resumed["repair"]["status"], "rolled-back");
    assert_eq!(
        revision(&cli(workspace.path(), &["get".into(), mission_id.clone()],)),
        compensated_revision,
        "recognizing the durable compensation must not append another revision"
    );
    assert_eq!(
        mission_payload(workspace.path(), &mission_id),
        compensated_payload
    );
}
