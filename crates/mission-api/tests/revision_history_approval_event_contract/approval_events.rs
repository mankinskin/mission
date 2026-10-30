use super::*;

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
