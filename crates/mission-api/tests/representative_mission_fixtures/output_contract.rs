use super::*;

#[test]
fn canonical_inputs_cover_native_and_typed_schemas_with_all_outcomes() {
    let corpus = fixture_paths();
    let documentation: NativeFixtureInput = serde_json::from_slice(
        &fs::read(corpus.join("authoring/documentation-workflow.native-v2.json")).unwrap(),
    )
    .unwrap();
    let ticket: NativeFixtureInput = serde_json::from_slice(
        &fs::read(corpus.join("authoring/ticket-reliability.native-v2.json")).unwrap(),
    )
    .unwrap();
    let integration: TypedDraft = serde_json::from_slice(
        &fs::read(corpus.join("authoring/mission-integration.typed-v3.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(
        documentation.bundle.manifest.schema_version,
        OUTCOME_BUNDLE_SCHEMA_VERSION
    );
    assert_eq!(documentation.bundle.waypoints.len(), DOC_SEEDS.len());
    assert_eq!(
        ticket.bundle.manifest.schema_version,
        OUTCOME_BUNDLE_SCHEMA_VERSION
    );
    assert_eq!(ticket.bundle.waypoints.len(), TICKET_SEEDS.len());
    assert_eq!(integration.schema_version, OUTCOME_SOURCE_SCHEMA_VERSION);
    assert_eq!(integration.waypoints.len(), MISSION_SEEDS.len());
    assert_eq!(
        serde_json::from_slice::<MissionBundle>(
            &serde_json::to_vec(&documentation.bundle).unwrap()
        )
        .unwrap(),
        documentation.bundle
    );
    assert_eq!(
        serde_json::from_slice::<MissionBundle>(&serde_json::to_vec(&ticket.bundle).unwrap())
            .unwrap(),
        ticket.bundle
    );
    assert_eq!(
        serde_json::from_slice::<TypedDraft>(&serde_json::to_vec(&integration).unwrap()).unwrap(),
        integration
    );
    for waypoint in documentation
        .bundle
        .waypoints
        .iter()
        .chain(&ticket.bundle.waypoints)
    {
        assert!(waypoint.outcome_plan.as_ref().is_some_and(|plan| {
            !plan.expected_outcome.trim().is_empty()
                && !plan.alternative_outcomes.is_empty()
                && plan.alternative_outcomes.iter().all(|alternative| {
                    !alternative.outcome.trim().is_empty()
                        && !alternative.acceptable_handling.is_empty()
                })
        }));
    }
    assert!(integration.waypoints.iter().all(|waypoint| {
        waypoint.outcome_plan.as_ref().is_some_and(|plan| {
            !plan.expected_outcome.trim().is_empty()
                && !plan.alternative_outcomes.is_empty()
                && plan.alternative_outcomes.iter().all(|alternative| {
                    !alternative.outcome.trim().is_empty()
                        && !alternative.acceptable_handling.is_empty()
                })
        })
    }));

    let completed = &ticket.bundle.waypoints[15];
    let pending = &ticket.bundle.waypoints[16];
    assert_eq!(completed.state, Some(WaypointState::Blocked));
    assert_eq!(pending.state, Some(WaypointState::Pending));
    assert!(
        ticket
            .bundle
            .waypoints
            .iter()
            .any(|waypoint| { waypoint.ticket_ref.is_none() && waypoint.relationships.is_empty() })
    );
    assert!(
        documentation
            .bundle
            .manifest
            .operator_results
            .iter()
            .any(|result| result.waypoint_id == "code-map")
    );
    for (slug, expected_parts) in [
        ("documentation-workflow", 4),
        ("mission-integration", 9),
        ("ticket-reliability", 17),
    ] {
        let waypoints = corpus
            .join("generated")
            .join(slug)
            .join("generated/waypoints");
        assert!(waypoints.join("README.md").is_file(), "{slug}");
        let part_count = fs::read_dir(waypoints)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some("md")
                    && entry.file_name() != "README.md"
            })
            .count();
        assert_eq!(part_count, expected_parts, "{slug}");
    }
}

#[test]
fn revision_history_retains_changed_request_continuations_and_synthetic_reaction() {
    let workspace = tempfile::Builder::new()
        .prefix(".representative-w8-history-")
        .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    let corpus = fixture_paths();
    let mission_id = generate_fixture(
        workspace.path(),
        &corpus.join("authoring/mission-integration.typed-v3.json"),
    );
    let mission = generated_directory(workspace.path(), mission_id);
    let revision_two = mission.join("history/states/revision-2");
    let current = mission;

    assert_eq!(
        fs::read_to_string(revision_two.join("REQUEST.md")).unwrap(),
        "Synthetic REQUEST state 2."
    );
    assert_eq!(
        fs::read_to_string(current.join("REQUEST.md")).unwrap(),
        "Changed synthetic REQUEST state 3."
    );
    assert!(revision_two.join("inputs/reviewer/initial.md").is_file());
    assert!(current.join("inputs/reviewer/initial.md").is_file());
    assert!(current.join("inputs/reviewer/follow-up.md").is_file());
    let event: MissionApprovalEvent =
        serde_json::from_slice(&fs::read(current.join("history/events/revision-1.json")).unwrap())
            .unwrap();
    assert_eq!(event.reaction.decision, ApprovalDecision::ChangesRequested);
    assert_eq!(
        event.event_id,
        Uuid::from_u128(0x0000000000004000800000000000f00d)
    );
    assert_eq!(event.reaction.actor_source, "fixture-authored");
    assert!(
        event
            .reaction
            .metadata
            .values()
            .any(|value| value.contains("not authentication"))
    );

    let store = MissionStore::open(workspace.path()).with_test_event_metadata(
        Uuid::from_u128(0x0000000000004000800000000000f00d),
        1_798_000_000_000,
    );
    store.verify_output_history(mission_id).unwrap();
    let contract = store.output_contract(mission_id).unwrap();
    assert_eq!(contract.revision, 3);
    assert!(contract.files.iter().any(|file| file.path == "REQUEST.md"));
    assert!(
        contract
            .files
            .iter()
            .any(|file| file.path == "inputs/reviewer/initial.md")
    );
}
