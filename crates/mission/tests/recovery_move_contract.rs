#[path = "support/direct_transports.rs"]
mod support;

use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
use support::*;
use uuid::Uuid;

const REPAIR_POLICY: &str = "forward-authoritative-repair";
const NATIVE_WAYPOINT_IDS: [&str; 12] = [
    "ownership-refactor",
    "typed-persistence",
    "adapters",
    "recovery-authority",
    "mission-root-resolution",
    "mission-repair",
    "mission-move",
    "session-set-transports",
    "session-schema-infrastructure",
    "migration",
    "migration-verification",
    "guidance-evidence",
];

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
            "test://repair/new".into(),
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

fn create_typed_fixture(workspace: &Path, add_old_evidence: bool) -> (String, u64) {
    let created = cli(
        workspace,
        &[
            "create-draft".into(),
            "Twelve waypoint repair fixture".into(),
            "Verify same-identity repair and typed rebind.".into(),
        ],
    );
    assert_ok(&created);
    let mission_id = created["mission_id"]
        .as_str()
        .expect("Mission ID")
        .to_owned();
    let mut waypoints = Vec::new();
    for (index, waypoint_id) in NATIVE_WAYPOINT_IDS.iter().enumerate() {
        let waypoint_id = (*waypoint_id).to_owned();
        let depends_on = if index == 0 {
            Vec::new()
        } else {
            vec![NATIVE_WAYPOINT_IDS[index - 1].to_owned()]
        };
        waypoints.push(json!({
            "waypoint_id": waypoint_id,
            "title": format!("Waypoint {index}"),
            "prompt": format!("Typed prompt {index}."),
            "part": {
                "waypoint_id": waypoint_id,
                "document": {
                    "blocks": [{
                        "type":"paragraph",
                        "inlines":[{"type":"text","text":format!("Part content {index}.")}]
                    }]
                }
            },
            "depends_on": depends_on,
            "validation": [{"command":"fixture validation","cwd":"."}],
            "commit_checkpoint": {"message":format!("Checkpoint {index}")},
            "non_goals": []
        }));
    }
    let draft = json!({
        "mission_id": mission_id,
        "schema_version": 2,
        "revision": 1,
        "title": "Twelve waypoint repair fixture",
        "objective": "Verify same-identity repair and typed rebind.",
        "waypoints": waypoints
    });
    let saved = cli(
        workspace,
        &[
            "save-draft".into(),
            mission_id.clone(),
            draft.to_string(),
            "1".into(),
        ],
    );
    assert_ok(&saved);
    let mut current_revision = revision(&saved);
    if add_old_evidence {
        for waypoint_id in NATIVE_WAYPOINT_IDS.iter().take(3) {
            let evidence = cli(
                workspace,
                &[
                    "record-validation-result".into(),
                    mission_id.clone(),
                    (*waypoint_id).into(),
                    "passed".into(),
                    "pre-repair validation".into(),
                    format!("test://repair/old/{waypoint_id}"),
                    current_revision.to_string(),
                ],
            );
            assert_ok(&evidence);
            current_revision = revision(&evidence);
            let completed = cli(
                workspace,
                &[
                    "record-progress".into(),
                    mission_id.clone(),
                    (*waypoint_id).into(),
                    "completed".into(),
                    current_revision.to_string(),
                ],
            );
            assert_ok(&completed);
            current_revision = revision(&completed);
        }
    }
    let current_source = cli(
        workspace,
        &["get-mission-source".into(), mission_id.clone()],
    );
    assert_ok(&current_source);
    let published = cli(
        workspace,
        &[
            "publish-draft".into(),
            mission_id.clone(),
            current_source["source"].to_string(),
            current_revision.to_string(),
        ],
    );
    assert_ok(&published);
    current_revision = revision(&published);
    if add_old_evidence {
        let source_path = workspace
            .join(".workflow-tools/mission/missions")
            .join(&mission_id)
            .join("source.json");
        let mut stored_source: Value =
            serde_json::from_slice(&fs::read(&source_path).expect("typed source")).unwrap();
        stored_source["revision"] = json!(current_revision - 1);
        fs::write(
            source_path,
            serde_json::to_vec_pretty(&stored_source).expect("serialized divergent source"),
        )
        .expect("create controlled revision divergence");
    }
    (mission_id, current_revision)
}

fn approved_preview_request(workspace: &Path, mission_id: &str) -> (Value, Value) {
    let root = workspace.to_string_lossy().into_owned();
    let initial = json!({
        "source_root": root,
        "policy": REPAIR_POLICY,
        "records": [{
            "mission_id": mission_id,
            "expected_revision": revision(&cli(workspace, &["get".into(), mission_id.into()])),
            "accepted_sha256": "",
            "source_sha256": "",
            "payload_fingerprint": ""
        }]
    });
    let discovery = run_repair_cli(
        workspace,
        "repair-preview",
        &repair_cli_args(&initial, None),
    );
    assert_eq!(discovery["status"], "error", "{discovery}");
    let observed = &discovery["repair"]["records"][0];
    let request = json!({
        "source_root": root,
        "policy": REPAIR_POLICY,
        "records": [{
            "mission_id": mission_id,
            "expected_revision": observed["observed_revision"],
            "accepted_sha256": observed["observed_accepted_sha256"],
            "source_sha256": observed["observed_source_sha256"],
            "payload_fingerprint": observed["payload_fingerprint"]
        }]
    });
    let preview = run_repair_cli(
        workspace,
        "repair-preview",
        &repair_cli_args(&request, None),
    );
    assert_eq!(preview["status"], "ok", "{preview}");
    (request, preview)
}

fn run_repair_cli(workspace: &Path, command: &str, args: &[String]) -> Value {
    let mut arguments = vec![command.to_owned()];
    arguments.extend(args.iter().cloned());
    let output = cli_output(
        &std::env::current_dir().expect("process cwd"),
        Some(workspace),
        None,
        &arguments,
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "repair CLI JSON expected ({error}); stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn repair_cli_args(request: &Value, proposal_id: Option<&str>) -> Vec<String> {
    let mut args = vec!["--records".into(), request.to_string()];
    if let Some(proposal_id) = proposal_id {
        args.push("--proposal-id".into());
        args.push(proposal_id.to_owned());
    }
    args
}

fn mission_payload(workspace: &Path, mission_id: &str) -> BTreeMap<String, Vec<u8>> {
    fn collect(root: &Path, current: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(current).expect("Mission payload directory") {
            let entry = entry.expect("Mission payload entry");
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, result);
            } else {
                result.insert(
                    path.strip_prefix(root)
                        .expect("relative Mission payload path")
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(path).expect("Mission payload bytes"),
                );
            }
        }
    }
    let root = workspace
        .join(".workflow-tools/mission/missions")
        .join(mission_id);
    let mut files = BTreeMap::new();
    collect(&root, &root, &mut files);
    files
}
