use super::*;

#[test]
fn waypoint_accepts_reference_only_optional_relationships() {
    let waypoint = json!({
        "waypoint_id": "verify",
        "title": "Verify",
        "prompt": "Run the check",
        "part": {"waypoint_id": "verify", "document": {"blocks": []}},
        "relationships": {
            "specification_ids": ["88a07686-34da-4ec9-a569-21eb286e7609"],
            "criteria": [{
                "specification_id": "88a07686-34da-4ec9-a569-21eb286e7609",
                "criterion_id": "mission-waypoint-link-optional"
            }],
            "executions": [{"execution_id": "fixture-green"}]
        }
    });
    assert!(
        serde_json::from_value::<TypedWaypoint>(waypoint).is_ok(),
        "Waypoints must accept reference-only Spec/criterion/execution relationships without a Ticket"
    );
}

#[test]
fn unpromoted_waypoint_is_valid_with_or_without_foreign_links() {
    for linked in [false, true] {
        let mut fixture = Fixture::new();
        if linked {
            fixture.link();
        }
        let published = MissionLifecycle::open(fixture.root())
            .publish_draft(fixture.draft.mission_id, fixture.draft.clone(), 1)
            .unwrap();
        let lifecycle = MissionLifecycle::open(fixture.root());
        let accepted = lifecycle.validate(published.mission_id).unwrap();
        assert!(accepted.bundle.waypoints[0].ticket_ref.is_none());
        assert!(accepted.bundle.waypoints[0].depends_on.is_empty());
        let roadmap =
            fs::read_to_string(fixture.mission_dir().join("generated/ROADMAP.md")).unwrap();
        if linked {
            assert!(roadmap.contains("Current canonical criterion"));
            assert!(roadmap.contains("fixture-green"));
        }
    }
}

#[test]
fn promotion_reads_existing_completed_ticket_without_activation_or_ownership_writes() {
    let mut fixture = Fixture::new();
    fixture.promote();
    fixture.link();
    let before_ticket = snapshot(&fixture.root().join(".workflow-tools/ticket"));
    let before_execution = snapshot(&fixture.root().join(".workflow-tools/test"));
    let saved = fixture.save().unwrap();
    let lifecycle = MissionLifecycle::open(fixture.root());
    let read = lifecycle.get_mission_source(saved.mission_id).unwrap();
    assert_eq!(
        read.waypoints[0].relationships,
        fixture.draft.waypoints[0].relationships
    );
    assert_eq!(
        read.waypoints[0].ownership_refs.ticket_id,
        Some(fixture.ticket.id)
    );
    let accepted = lifecycle.validate(saved.mission_id).unwrap();
    assert!(accepted.bundle.waypoints[0].state.is_none());
    assert!(accepted.bundle.waypoints[0].depends_on.is_empty());
    assert_eq!(
        snapshot(&fixture.root().join(".workflow-tools/ticket")),
        before_ticket
    );
    assert_eq!(
        snapshot(&fixture.root().join(".workflow-tools/test")),
        before_execution
    );
    let source = fs::read_to_string(fixture.mission_dir().join("source.json")).unwrap();
    assert!(!source.contains("Current canonical criterion"));
    assert!(!source.contains("Current execution detail"));
}

#[test]
fn execution_accepts_optional_criterion_and_rejects_mismatched_canonical_link() {
    let mut fixture = Fixture::new();
    fixture.link();
    fixture.draft.waypoints[0].relationships.executions[0].criterion =
        Some(fixture.criterion.clone());
    assert!(fixture.save().is_ok());
    fixture.draft.revision = 2;
    fixture.draft.waypoints[0].relationships.executions[0]
        .criterion
        .as_mut()
        .unwrap()
        .criterion_id = CriterionId::new("unrelated-criterion").unwrap();
    fixture.assert_rejected_unchanged();
}
