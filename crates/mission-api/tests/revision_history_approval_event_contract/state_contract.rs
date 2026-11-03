use super::*;

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
    assert!(
        String::from_utf8(first.clone())
            .unwrap()
            .contains("renderer: mission-api/5")
    );
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
        include_str!("../snapshots/revision-history-1-and-3.md.snap")
    );
    store.verify_output_history(id).unwrap();
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
fn pure_preview_does_not_invent_available_historical_states() {
    let rendered = render(&bundle(100), Uuid::nil()).unwrap();
    let history = String::from_utf8_lossy(&rendered.parts["HISTORY.md"]);
    assert!(history.contains("archive availability is unknown"));
    assert!(!history.contains("available complete state"));
}
