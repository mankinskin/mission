use super::*;

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
