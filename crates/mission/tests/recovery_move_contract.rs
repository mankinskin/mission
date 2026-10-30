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
                    format!("ce://repair/test/old-{waypoint_id}"),
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

#[path = "recovery_move_contract/repair_apply.rs"]
mod repair_apply;
#[path = "recovery_move_contract/repair_resume.rs"]
mod repair_resume;
#[path = "recovery_move_contract/repair_rollback.rs"]
mod repair_rollback;
