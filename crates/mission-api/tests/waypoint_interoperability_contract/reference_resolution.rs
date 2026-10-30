use super::*;

#[test]
fn missing_wrong_root_and_dangling_links_preserve_all_mission_authority() {
    for invalid in [
        "missing-ticket",
        "wrong-ticket-root",
        "nil-ticket",
        "missing-spec",
        "dangling-criterion",
        "missing-execution",
        "execution-path",
        "wrong-spec-root",
    ] {
        let mut fixture = Fixture::new();
        fixture.promote();
        fixture.link();
        match invalid {
            "missing-ticket" => {
                fixture.draft.waypoints[0].ownership_refs.ticket_id = Some(Uuid::new_v4())
            }
            "wrong-ticket-root" => {
                fixture.draft.waypoints[0].ownership_refs.ticket_id = None;
                fixture.draft.waypoints[0].ownership_refs.ticket_uri =
                    Some(format!("ce://other/ticket/{}", fixture.ticket.id));
            }
            "nil-ticket" => fixture.draft.waypoints[0].ownership_refs.ticket_id = Some(Uuid::nil()),
            "missing-spec" => {
                fixture.draft.waypoints[0].relationships.specification_ids = vec![Uuid::new_v4()]
            }
            "dangling-criterion" => {
                fixture.draft.waypoints[0].relationships.criteria[0].criterion_id =
                    CriterionId::new("absent").unwrap()
            }
            "missing-execution" => {
                fixture.draft.waypoints[0].relationships.executions[0].execution_id =
                    "absent".into()
            }
            "execution-path" => {
                fixture.draft.waypoints[0].relationships.executions[0].execution_id =
                    "../escape".into()
            }
            "wrong-spec-root" => {
                let ticket_path = fixture
                    .root()
                    .join(".workflow-tools/ticket/tickets")
                    .join(fixture.ticket.id.to_string());
                TicketFs::update(&ticket_path, &[("related_specs".into(), json!([{
                            "spec_id": fixture.spec.id, "workspace": "other", "store_root": "nested/.workflow-tools/spec"
                        }]))].into(), None).unwrap();
            }
            _ => unreachable!(),
        }
        fixture.assert_rejected_unchanged();
    }
}

#[test]
fn generation_reads_current_ticket_specification_criterion_and_execution_content() {
    let mut fixture = Fixture::new();
    fixture.promote();
    fixture.link();
    let saved = fixture.save().unwrap();
    let lifecycle = MissionLifecycle::open(fixture.root());
    let accepted = lifecycle.get(saved.mission_id).unwrap();
    let before = snapshot(&fixture.mission_dir());
    let ticket_path = fixture
        .root()
        .join(".workflow-tools/ticket/tickets")
        .join(fixture.ticket.id.to_string());
    TicketFs::update(
        &ticket_path,
        &[("title".into(), json!("Updated Ticket"))].into(),
        None,
    )
    .unwrap();
    let mut store = SpecStore::open(&fixture.root().join(".workflow-tools/spec")).unwrap();
    store
        .update(
            &fixture.spec.id.to_string(),
            [
                ("title".into(), json!("Updated specification")),
                (
                    "acceptance_criteria".into(),
                    json!([{
                        "id": fixture.criterion.criterion_id.as_str(),
                        "statement": "Updated canonical criterion",
                        "expected_property_ids": [], "required_evidence_ids": []
                    }]),
                ),
            ]
            .into(),
            None,
        )
        .unwrap();
    store
        .update_body(
            &fixture.spec.id.to_string(),
            "Updated specification body",
            false,
        )
        .unwrap();
    let tests = TestStoreConfig::new(fixture.root().join(".workflow-tools/test"));
    let mut execution = tests.get_execution("fixture-green").unwrap();
    execution.detail = Some("Updated execution".into());
    execution.outcome = test_api::ValidationOutcome::Failed;
    tests.record_execution(&execution).unwrap();
    let rendered =
        render_in_workspace(fixture.root(), &accepted.bundle, accepted.mission_id).unwrap();
    let roadmap = String::from_utf8(rendered.roadmap).unwrap();
    for content in [
        "Updated Ticket",
        "Updated specification",
        "Updated specification body",
        "Updated canonical criterion",
        "Updated execution",
        "Failed",
        "fixture-green",
        &fixture.ticket.id.to_string(),
        &fixture.spec.id.to_string(),
    ] {
        assert!(roadmap.contains(content), "missing {content}: {roadmap}");
    }
    assert!(roadmap.contains(
        "Owning workspace: selected workspace root (local records in .workflow-tools/)."
    ));
    let canonical_root = fixture.root().canonicalize().unwrap();
    assert!(!roadmap.contains(&canonical_root.to_string_lossy().to_string()));
    assert!(roadmap.contains("renderer: mission-api/7"));
    let part = String::from_utf8(rendered.parts["waypoints/01-verify.md"].clone()).unwrap();
    assert!(part.contains("Updated Ticket"));
    assert!(part.contains(
        "Owning workspace: selected workspace root (local records in .workflow-tools/)."
    ));
    assert!(!part.contains(&canonical_root.to_string_lossy().to_string()));
    assert!(
        lifecycle
            .render_mission(saved.mission_id)
            .unwrap()
            .contains("Updated Ticket")
    );
    assert_eq!(snapshot(&fixture.mission_dir()), before);
}

#[test]
fn linked_v7_publication_preserves_unlinked_v6_history() {
    let mut fixture = Fixture::new();
    let mission_id = fixture.draft.mission_id;
    let lifecycle = MissionLifecycle::open(fixture.root());
    lifecycle
        .publish_draft(mission_id, fixture.draft.clone(), 1)
        .unwrap();
    let directory = fixture.mission_dir();
    let store = MissionStore::open(fixture.root());
    let accepted = store.get(mission_id).unwrap();
    let mut unlinked_v2 = accepted.bundle.clone();
    unlinked_v2.manifest.schema_version = OUTCOME_BUNDLE_SCHEMA_VERSION;
    let unlinked_revision = accepted.bundle.manifest.revision + 1;
    unlinked_v2.manifest.revision = unlinked_revision;
    for waypoint in &mut unlinked_v2.waypoints {
        waypoint.outcome_plan = Some(WaypointOutcomePlan {
            expected_outcome: "Fixture-authored migration outcome".into(),
            alternative_outcomes: vec![AlternativeOutcome {
                outcome: "Fixture validation reports a problem".into(),
                acceptable_handling: vec!["Record the observation and preserve the state.".into()],
            }],
        });
    }
    store
        .publish(
            mission_id,
            unlinked_v2.clone(),
            accepted.bundle.manifest.revision,
        )
        .unwrap();
    let original_roadmap = fs::read(directory.join("generated/ROADMAP.md")).unwrap();
    let original_contract = store.output_contract(mission_id).unwrap();
    assert_eq!(original_contract.renderer, "mission-api/6");
    assert!(
        String::from_utf8(original_roadmap.clone())
            .unwrap()
            .contains("renderer: mission-api/6")
    );

    let mut linked_v2 = unlinked_v2;
    linked_v2.manifest.revision = unlinked_revision + 1;
    linked_v2.waypoints[0].relationships = WaypointRelationships {
        specification_ids: vec![fixture.spec.id],
        criteria: vec![fixture.criterion.clone()],
        executions: vec![WaypointExecutionReference {
            execution_id: "fixture-green".into(),
            criterion: None,
        }],
    };
    store
        .publish(mission_id, linked_v2, unlinked_revision)
        .unwrap();

    let contract = store.output_contract(mission_id).unwrap();
    assert_eq!(contract.renderer, "mission-api/7");
    assert!(
        fs::read_to_string(directory.join("generated/ROADMAP.md"))
            .unwrap()
            .contains("renderer: mission-api/7")
    );
    let archived_contract_path = directory
        .join("history/states")
        .join(format!("revision-{unlinked_revision}/output-contract.json"));
    let archived_contract: MissionOutputContract =
        serde_json::from_slice(&fs::read(archived_contract_path).unwrap()).unwrap();
    assert_eq!(archived_contract.renderer, "mission-api/6");
    assert_eq!(
        fs::read(
            directory
                .join("history/states")
                .join(format!("revision-{unlinked_revision}/generated/ROADMAP.md"))
        )
        .unwrap(),
        original_roadmap
    );
    store.verify_output_history(mission_id).unwrap();
}

#[test]
fn deleted_entity_prevents_validation_render_and_publication_without_source_writes() {
    let mut fixture = Fixture::new();
    fixture.link();
    let published = MissionLifecycle::open(fixture.root())
        .publish_draft(fixture.draft.mission_id, fixture.draft.clone(), 1)
        .unwrap();
    let before = snapshot(&fixture.mission_dir());
    fs::remove_file(
        fixture
            .root()
            .join(".workflow-tools/test/executions/fixture-green.json"),
    )
    .unwrap();
    let lifecycle = MissionLifecycle::open(fixture.root());
    assert!(lifecycle.validate(published.mission_id).is_err());
    assert!(lifecycle.render_mission(published.mission_id).is_err());
    let mut next = published.clone();
    next.revision = 3;
    assert!(lifecycle.publish_draft(next.mission_id, next, 2).is_err());
    assert_eq!(snapshot(&fixture.mission_dir()), before);
}
