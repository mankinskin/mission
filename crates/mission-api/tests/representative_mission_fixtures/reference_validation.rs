use super::*;

#[test]
fn invalid_reference_drift_and_malformed_plan_preserve_accepted_files() {
    let workspace = tempfile::Builder::new()
        .prefix(".representative-w8-negative-")
        .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    let corpus = fixture_paths();
    let authoring = corpus.join("authoring/documentation-workflow.native-v2.json");
    let input: NativeFixtureInput = serde_json::from_slice(&fs::read(&authoring).unwrap()).unwrap();
    let mission_id = generate_fixture(workspace.path(), &authoring);
    let store = MissionStore::open(workspace.path());
    let directory = generated_directory(workspace.path(), mission_id);

    let before = tree(&directory);
    let mut missing_reference = store.get(mission_id).unwrap().bundle;
    missing_reference.manifest.revision += 1;
    let linked = missing_reference
        .waypoints
        .iter_mut()
        .find(|waypoint| !waypoint.relationships.is_empty())
        .unwrap();
    linked.relationships.specification_ids[0] = Uuid::from_u128(0x0000000000004000800000000000dead);
    let error = store.publish(mission_id, missing_reference, 1).unwrap_err();
    assert_diagnostic(error, "mission-waypoint-link-unavailable");
    assert_eq!(
        tree(&directory),
        before,
        "missing reference changed accepted bytes"
    );

    let generated = directory.join("generated/README.md");
    let mut drifted = fs::read(&generated).unwrap();
    drifted.extend_from_slice(b"\nsynthetic drift\n");
    fs::write(&generated, &drifted).unwrap();
    let drifted_tree = tree(&directory);
    let error = store.verify_output_history(mission_id).unwrap_err();
    assert_diagnostic(error, "mission-output-history-invalid");
    assert_eq!(
        tree(&directory),
        drifted_tree,
        "drift validation rewrote files"
    );
    fs::write(&generated, before.get("generated/README.md").unwrap()).unwrap();

    let mut malformed = input.bundle;
    malformed.manifest.revision = 2;
    malformed.waypoints[0]
        .outcome_plan
        .as_mut()
        .unwrap()
        .expected_outcome
        .clear();
    let before_rejection = tree(&directory);
    let error = store.publish(mission_id, malformed, 1).unwrap_err();
    assert_diagnostic(error, "invalid-outcome-plan");
    assert_eq!(tree(&directory), before_rejection);
}
