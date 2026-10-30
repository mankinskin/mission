use super::*;

#[test]
fn repair_resume_and_rollback_reject_later_unrelated_payload_edits() {
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
    let unrelated = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(&mission_id)
        .join("unrelated-edit.txt");
    fs::write(&unrelated, b"written after repair").expect("write unrelated edit");
    let edited_payload = mission_payload(workspace.path(), &mission_id);

    let resumed = cli(
        workspace.path(),
        &[
            "repair-resume".into(),
            "--journal-id".into(),
            journal_id.clone(),
        ],
    );
    assert_eq!(resumed["status"], "error", "{resumed}");
    assert_eq!(resumed["diagnostics"][0]["code"], "repair-resume-drift");
    assert_eq!(
        mission_payload(workspace.path(), &mission_id),
        edited_payload
    );

    let rolled_back = cli(
        workspace.path(),
        &["repair-rollback".into(), "--journal-id".into(), journal_id],
    );
    assert_eq!(rolled_back["repair"]["status"], "blocked", "{rolled_back}");
    assert!(
        rolled_back["repair"]["diagnostics"][0]
            .as_str()
            .is_some_and(|diagnostic| diagnostic.contains("repair-rollback-drift"))
    );
    assert_eq!(
        mission_payload(workspace.path(), &mission_id),
        edited_payload
    );
}

#[test]
fn repair_resume_rejects_missing_and_corrupt_journals_without_mission_writes() {
    for corrupt in [false, true] {
        let workspace = TempDir::new().expect("workspace");
        let (mission_id, _) = create_typed_fixture(workspace.path(), true);
        let before = mission_payload(workspace.path(), &mission_id);
        let journal_id = Uuid::new_v4();
        let journal_path = workspace
            .path()
            .join(".workflow-tools/mission/.repair-journals")
            .join(format!("{journal_id}.json"));
        if corrupt {
            fs::create_dir_all(journal_path.parent().expect("journal parent"))
                .expect("create journal directory");
            fs::write(&journal_path, b"{invalid json").expect("write corrupt journal");
        }

        let rejected = cli(
            workspace.path(),
            &[
                "repair-resume".into(),
                "--journal-id".into(),
                journal_id.to_string(),
            ],
        );
        assert_eq!(rejected["status"], "error", "{rejected}");
        assert_eq!(mission_payload(workspace.path(), &mission_id), before);
        if corrupt {
            assert_eq!(
                fs::read(&journal_path).expect("preserved corrupt journal"),
                b"{invalid json"
            );
        } else {
            assert!(!journal_path.exists());
        }
    }
}
