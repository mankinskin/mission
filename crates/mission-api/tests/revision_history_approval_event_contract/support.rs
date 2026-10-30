use super::*;

pub(crate) fn bundle(revision: u64) -> MissionBundle {
    MissionBundle {
        manifest: BundleManifest {
            schema_version: 1,
            revision,
            title: "History".into(),
            objective: "Preserve complete states".into(),
            waypoint_files: vec![],
            execution_order: vec!["first".into()],
            artifacts: vec![],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
            operator_results: vec![],
        },
        waypoints: vec![WaypointFragment {
            outcome_plan: None,
            waypoint_id: "first".into(),
            title: "First".into(),
            depends_on: vec![],
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            child_mission_ref: None,
            relationships: Default::default(),
            session_package: "first".into(),
            prompt: "Implement".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "true".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "feat: first".into(),
            },
            part_markdown: format!("State {revision}"),
        }],
    }
}

pub(crate) fn fixture() -> (tempfile::TempDir, MissionStore, Uuid, PathBuf) {
    let workspace = tempfile::Builder::new()
        .prefix("w6-contract-")
        .tempdir_in(env!("CARGO_MANIFEST_DIR"))
        .unwrap();
    let store = MissionStore::init(workspace.path()).unwrap();
    let id = Uuid::new_v4();
    let directory = workspace
        .path()
        .join(".workflow-tools/mission/missions")
        .join(id.to_string());
    (workspace, store, id, directory)
}
pub(crate) fn tree(directory: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
    fn collect(
        root: &std::path::Path,
        directory: &std::path::Path,
        result: &mut BTreeMap<String, Vec<u8>>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                collect(root, &entry.path(), result);
            } else {
                result.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .to_string(),
                    fs::read(entry.path()).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    collect(directory, directory, &mut result);
    result
}

pub(crate) fn reaction(
    directory: &std::path::Path,
    revision: u64,
    decision: &str,
) -> serde_json::Value {
    serde_json::json!({
        "revision": revision,
        "decision": decision,
        "signature": format!("sha256:{:x}", Sha256::digest(fs::read(directory.join("output-contract.json")).unwrap())),
        "actor": "requester",
        "actor_source": "explicit synthetic test input",
        "metadata": {"channel": "review", "note": "No product readiness claim"}
    })
}
