use mission_api::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{fs, path::PathBuf};
use uuid::Uuid;

fn bundle(revision: u64) -> MissionBundle {
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

fn fixture() -> (tempfile::TempDir, MissionStore, Uuid, PathBuf) {
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

#[test]
fn publication_preserves_whole_states_at_shared_revision_labels() {
    let (_workspace, store, id, directory) = fixture();
    store.publish(id, bundle(1), 0).unwrap();
    assert!(directory.join("generated/waypoints/01-first.md").is_file());
    assert!(
        directory
            .join("history/states/revision-1/generated/waypoints/01-first.md")
            .is_file()
    );
    let first = fs::read(directory.join("generated/ROADMAP.md")).unwrap();
    assert!(String::from_utf8(first.clone())
        .unwrap()
        .contains("renderer: mission-api/5"));
    store.publish(id, bundle(3), 1).unwrap();
    assert_eq!(
        fs::read(directory.join("history/states/revision-1/generated/ROADMAP.md")).unwrap(),
        first
    );
    let history = fs::read_to_string(directory.join("generated/HISTORY.md")).unwrap();
    assert!(history.contains("revision-1"));
    assert!(history.contains("revision-2: unrecoverable gap"));
    assert!(history.contains("revision-3"));
    assert_eq!(
        history,
        include_str!("snapshots/revision-history-1-and-3.md")
    );
    store.verify_output_history(id).unwrap();
}

fn tree(directory: &std::path::Path) -> BTreeMap<String, Vec<u8>> {
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

fn reaction(directory: &std::path::Path, revision: u64, decision: &str) -> serde_json::Value {
    serde_json::json!({
        "revision": revision,
        "decision": decision,
        "signature": format!("sha256:{:x}", Sha256::digest(fs::read(directory.join("output-contract.json")).unwrap())),
        "actor": "requester",
        "actor_source": "explicit synthetic test input",
        "metadata": {"channel": "review", "note": "No product readiness claim"}
    })
}

#[test]
fn contract_bom_matches_all_canonical_bytes_and_archived_state() {
    let (_workspace, store, id, directory) = fixture();
    let inputs = MissionOutputInputs {
        merged_request: Some("Merged request\n".into()),
        continuations: BTreeMap::from([
            ("first.md".into(), "First independent input\n".into()),
            ("second.md".into(), "Second independent input\n".into()),
        ]),
    };
    store
        .publish_with_inputs(id, bundle(1), 0, &inputs, None)
        .unwrap();
    let current = tree(&directory)
        .into_iter()
        .filter(|(name, _)| !name.starts_with("history/"))
        .collect::<BTreeMap<_, _>>();
    let archived = tree(&directory.join("history/states/revision-1"));
    assert_eq!(current, archived);
    let contract = store.output_contract(id).unwrap();
    assert_eq!(contract.revision_label, "revision-1");
    assert_eq!(contract.renderer, "mission-api/5");
    assert_eq!(contract.producer, "mission-api");
    assert_eq!(
        contract
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<Vec<_>>(),
        vec![
            "REQUEST.md",
            "accepted.json",
            "artifact-provenance.json",
            "generated/HISTORY.md",
            "generated/README.md",
            "generated/ROADMAP.md",
            "generated/waypoints/01-first.md",
            "generated/waypoints/README.md",
            "inputs/first.md",
            "inputs/second.md",
        ]
    );
    for file in &contract.files {
        let bytes = &current[&file.path];
        assert_eq!(file.size_bytes, bytes.len() as u64);
        assert_eq!(file.sha256, format!("{:x}", Sha256::digest(bytes)));
        assert!(!file.role.is_empty() && !file.format.is_empty());
    }
    assert_eq!(
        fs::read_to_string(directory.join("REQUEST.md")).unwrap(),
        "Merged request\n"
    );
    assert_eq!(
        fs::read_to_string(directory.join("inputs/second.md")).unwrap(),
        "Second independent input\n"
    );
    assert!(
        fs::read_to_string(directory.join("generated/README.md"))
            .unwrap()
            .contains("(waypoints/01-first.md)")
    );
    assert!(
        fs::read_to_string(directory.join("generated/ROADMAP.md"))
            .unwrap()
            .contains("(waypoints/01-first.md)")
    );
    assert_eq!(
        fs::read_to_string(directory.join("generated/waypoints/README.md")).unwrap(),
        "# Current waypoint Parts\n\n- [01-first.md](01-first.md)\n"
    );
    store.verify_output_history(id).unwrap();
}

#[test]
fn both_reactions_are_separate_create_new_records_and_never_advance_authority() {
    for decision in ["approve", "changes-requested"] {
        let (_workspace, store, id, directory) = fixture();
        store.publish(id, bundle(1), 0).unwrap();
        let authority = fs::read(directory.join("accepted.json")).unwrap();
        let archive = tree(&directory.join("history/states"));
        let request = reaction(&directory, 1, decision);
        let event = store
            .record_approval_event_json(id, &serde_json::to_vec(&request).unwrap())
            .unwrap();
        assert_eq!(store.approval_event(id, 1).unwrap(), event);
        assert!(event.recorded_at_unix_ms > 0 && !event.event_id.is_nil());
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["decision"], decision);
        assert_eq!(json["signature"], request["signature"]);
        assert_eq!(json["metadata"], request["metadata"]);
        assert_eq!(json["actor_source"], request["actor_source"]);
        assert!(json.get("files").is_none());
        assert_eq!(json.as_object().unwrap().len(), 10);
        assert_eq!(
            fs::read(directory.join("accepted.json")).unwrap(),
            authority
        );
        assert_eq!(tree(&directory.join("history/states")), archive);
        let before = tree(&directory);
        assert!(
            store
                .record_approval_event_json(id, &serde_json::to_vec(&request).unwrap())
                .is_err()
        );
        assert_eq!(tree(&directory), before);
        store.publish(id, bundle(2), 1).unwrap();
        assert_eq!(store.approval_event(id, 1).unwrap(), event);
        assert!(store.approval_event(id, 2).is_err());
        assert!(
            store
                .record_approval_event_json(id, &serde_json::to_vec(&request).unwrap())
                .is_err()
        );
        store.verify_output_history(id).unwrap();
    }
}

#[test]
fn invalid_reactions_fail_without_creating_or_changing_records() {
    let (_workspace, store, id, directory) = fixture();
    store.publish(id, bundle(1), 0).unwrap();
    let valid = reaction(&directory, 1, "approve");
    let mut invalid = vec![b"{".to_vec()];
    for (field, value) in [
        ("decision", serde_json::json!("accept")),
        ("signature", serde_json::json!("sha256:not-the-contract")),
        ("actor", serde_json::json!("")),
        ("actor_source", serde_json::json!("")),
        ("revision", serde_json::json!(0)),
        ("files", serde_json::json!([])),
        ("recorded_at_unix_ms", serde_json::json!(1)),
        ("event_id", serde_json::json!(Uuid::new_v4())),
    ] {
        let mut input = valid.clone();
        input[field] = value;
        invalid.push(serde_json::to_vec(&input).unwrap());
    }
    for input in invalid {
        let before = tree(&directory);
        assert!(
            store.record_approval_event_json(id, &input).is_err(),
            "{input:?}"
        );
        assert_eq!(tree(&directory), before);
    }
    fs::remove_file(directory.join("output-contract.json")).unwrap();
    let before = tree(&directory);
    assert!(
        store
            .record_approval_event_json(id, &serde_json::to_vec(&valid).unwrap())
            .is_err()
    );
    assert_eq!(tree(&directory), before);
}

#[test]
fn contract_json_digest_size_inventory_and_archive_tampering_are_rejected() {
    for mutation in [
        "json",
        "sha256",
        "size_bytes",
        "missing-entry",
        "extra-file",
        "archive",
    ] {
        let (_workspace, store, id, directory) = fixture();
        store.publish(id, bundle(1), 0).unwrap();
        let path = directory.join("output-contract.json");
        match mutation {
            "json" => fs::write(&path, "{").unwrap(),
            "extra-file" => fs::write(directory.join("extra.md"), "unexpected").unwrap(),
            "archive" => fs::write(
                directory.join("history/states/revision-1/generated/ROADMAP.md"),
                "tampered",
            )
            .unwrap(),
            _ => {
                let mut contract: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                if mutation == "missing-entry" {
                    contract["files"].as_array_mut().unwrap().pop();
                } else {
                    contract["files"][0][mutation] = if mutation == "sha256" {
                        serde_json::json!("bad")
                    } else {
                        serde_json::json!(0)
                    };
                }
                fs::write(&path, serde_json::to_vec_pretty(&contract).unwrap()).unwrap();
            }
        }
        let before = tree(&directory);
        assert!(store.verify_output_history(id).is_err(), "{mutation}");
        let request = reaction(&directory, 1, "approve");
        assert!(
            store
                .record_approval_event_json(id, &serde_json::to_vec(&request).unwrap())
                .is_err()
        );
        assert_eq!(tree(&directory), before);
    }
}

#[test]
fn occupied_revision_and_input_collisions_preserve_complete_previous_state() {
    for collision in ["state", "accepted-history", "input", "unsafe-input"] {
        let (_workspace, store, id, directory) = fixture();
        let mut inputs = MissionOutputInputs {
            merged_request: Some("Merged".into()),
            continuations: BTreeMap::from([("one.md".into(), "Original".into())]),
        };
        store
            .publish_with_inputs(id, bundle(1), 0, &inputs, None)
            .unwrap();
        inputs.continuations.clear();
        match collision {
            "state" => fs::create_dir(directory.join("history/states/revision-2")).unwrap(),
            "accepted-history" => {
                fs::write(directory.join("history/revision-2.json"), "occupied").unwrap()
            }
            "input" => {
                inputs
                    .continuations
                    .insert("one.md".into(), "Replacement".into());
            }
            _ => {
                inputs
                    .continuations
                    .insert("../accepted.json".into(), "Escape".into());
            }
        }
        let before = tree(&directory);
        assert!(
            store
                .publish_with_inputs(id, bundle(2), 1, &inputs, None)
                .is_err()
        );
        assert_eq!(tree(&directory), before);
        assert_eq!(store.get(id).unwrap().bundle.manifest.revision, 1);
    }
}

#[test]
fn absent_old_states_are_explicit_gaps_never_fabricated() {
    let (_workspace, store, id, directory) = fixture();
    store.publish(id, bundle(1), 0).unwrap();
    fs::remove_dir_all(directory.join("history/states/revision-1")).unwrap();
    let original_accepted = fs::read(directory.join("history/revision-1.json")).unwrap();
    store.publish(id, bundle(3), 1).unwrap();
    assert!(!directory.join("history/states/revision-1").exists());
    assert_eq!(
        fs::read(directory.join("history/revision-1.json")).unwrap(),
        original_accepted
    );
    assert!(
        fs::read_to_string(directory.join("generated/HISTORY.md"))
            .unwrap()
            .contains("revision-1..revision-2: unrecoverable gap")
    );
    store.verify_output_history(id).unwrap();
}

#[test]
fn typed_migration_and_forward_compensation_preserve_every_state_and_reaction() {
    let (workspace, store, id, directory) = fixture();
    store.publish(id, bundle(1), 0).unwrap();
    let request = reaction(&directory, 1, "changes-requested");
    let event = store
        .record_approval_event_json(id, &serde_json::to_vec(&request).unwrap())
        .unwrap();
    let original = tree(&directory.join("history"));
    let mut journal = MissionLifecycle::plan_migration(&[workspace.path().to_path_buf()]).unwrap();
    MissionLifecycle::apply_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::Applied);
    let contract = store.output_contract(id).unwrap();
    assert!(contract.files.iter().any(|file| file.path == "source.json"));
    assert!(
        contract
            .files
            .iter()
            .any(|file| file.path == "parts/first.json")
    );
    let migrated = tree(&directory.join("history"));
    for (name, bytes) in original {
        assert_eq!(migrated[&name], bytes);
    }
    MissionLifecycle::rollback_migration(&mut journal).unwrap();
    assert_eq!(journal.records[0].state, MigrationState::RolledBack);
    assert_eq!(store.get(id).unwrap().bundle.manifest.revision, 3);
    assert!(!directory.join("source.json").exists());
    let compensated = tree(&directory.join("history"));
    for (name, bytes) in migrated {
        assert_eq!(compensated[&name], bytes);
    }
    assert_eq!(store.approval_event(id, 1).unwrap(), event);
    assert!(store.approval_event(id, 3).is_err());
    store.verify_output_history(id).unwrap();
}

#[test]
fn additive_inputs_stay_independent_across_revisions() {
    let (_workspace, store, id, directory) = fixture();
    store
        .publish_with_inputs(
            id,
            bundle(1),
            0,
            &MissionOutputInputs {
                merged_request: Some("First merged request".into()),
                continuations: BTreeMap::from([("one.md".into(), "First input".into())]),
            },
            None,
        )
        .unwrap();
    store
        .publish_with_inputs(
            id,
            bundle(2),
            1,
            &MissionOutputInputs {
                merged_request: Some("Merged request with continuation".into()),
                continuations: BTreeMap::from([("two.md".into(), "Second input".into())]),
            },
            None,
        )
        .unwrap();
    assert_eq!(
        fs::read(directory.join("inputs/one.md")).unwrap(),
        b"First input"
    );
    assert_eq!(
        fs::read(directory.join("inputs/two.md")).unwrap(),
        b"Second input"
    );
    assert!(!directory.join("one.md").exists());
    assert!(
        !directory
            .join("history/states/revision-1/inputs/two.md")
            .exists()
    );
    assert_eq!(
        fs::read(directory.join("history/states/revision-1/REQUEST.md")).unwrap(),
        b"First merged request"
    );
    store.verify_output_history(id).unwrap();
}

#[test]
fn malformed_preserved_reactions_block_publication_without_repairing_history() {
    let (_workspace, store, id, directory) = fixture();
    store.publish(id, bundle(1), 0).unwrap();
    let request = reaction(&directory, 1, "approve");
    store
        .record_approval_event_json(id, &serde_json::to_vec(&request).unwrap())
        .unwrap();
    fs::write(directory.join("history/events/revision-1.json"), "{invalid").unwrap();
    let before = tree(&directory);
    assert!(store.publish(id, bundle(2), 1).is_err());
    assert!(store.verify_output_history(id).is_err());
    assert_eq!(tree(&directory), before);
}

#[test]
fn pure_preview_does_not_invent_available_historical_states() {
    let rendered = render(&bundle(100), Uuid::nil()).unwrap();
    let history = String::from_utf8_lossy(&rendered.parts["HISTORY.md"]);
    assert!(history.contains("archive availability is unknown"));
    assert!(!history.contains("available complete state"));
}
