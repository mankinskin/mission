use super::*;

#[test]
fn entities_in_other_workspace_do_not_satisfy_local_links() {
    let fixture = Fixture::new();
    let other = Fixture::new();
    let mut draft = other.draft.clone();
    draft.waypoints[0].relationships.specification_ids = vec![fixture.spec.id];
    assert!(
        MissionLifecycle::open(other.root())
            .save_draft(draft.mission_id, draft, 1)
            .is_err()
    );
}

#[cfg(unix)]
#[test]
fn symlinked_ticket_spec_and_execution_are_wrong_root_even_inside_workspace() {
    for kind in ["ticket", "spec", "execution"] {
        let mut fixture = Fixture::new();
        fixture.promote();
        fixture.link();
        let path = match kind {
            "ticket" => fixture
                .root()
                .join(".workflow-tools/ticket/tickets")
                .join(fixture.ticket.id.to_string()),
            "spec" => fixture
                .root()
                .join(".workflow-tools/spec/specs")
                .join(fixture.spec.id.to_string()),
            _ => fixture
                .root()
                .join(".workflow-tools/test/executions/fixture-green.json"),
        };
        let relocated = fixture.root().join(format!("relocated-{kind}"));
        fs::rename(&path, &relocated).unwrap();
        std::os::unix::fs::symlink(&relocated, &path).unwrap();
        fixture.assert_rejected_unchanged();
    }
}

#[test]
fn mismatched_entity_id_and_duplicate_links_are_rejected() {
    for invalid in [
        "ticket-id",
        "spec-id",
        "execution-id",
        "duplicate-spec",
        "duplicate-criterion",
        "duplicate-execution",
    ] {
        let mut fixture = Fixture::new();
        fixture.promote();
        fixture.link();
        match invalid {
            "ticket-id" => {
                let path = fixture
                    .root()
                    .join(".workflow-tools/ticket/tickets")
                    .join(fixture.ticket.id.to_string())
                    .join("ticket.toml");
                let mut ticket = fixture.ticket.clone();
                ticket.id = Uuid::new_v4();
                fs::write(path, toml::to_string(&ticket).unwrap()).unwrap();
            }
            "spec-id" => {
                let path = fixture
                    .root()
                    .join(".workflow-tools/spec/specs")
                    .join(fixture.spec.id.to_string())
                    .join("spec.toml");
                let mut spec = fixture.spec.clone();
                spec.id = Uuid::new_v4();
                fs::write(path, toml::to_string(&spec).unwrap()).unwrap();
            }
            "execution-id" => {
                let path = fixture
                    .root()
                    .join(".workflow-tools/test/executions/fixture-green.json");
                let mut value: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                value["id"] = json!("another-execution");
                fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            "duplicate-spec" => fixture.draft.waypoints[0]
                .relationships
                .specification_ids
                .push(fixture.spec.id),
            "duplicate-criterion" => fixture.draft.waypoints[0]
                .relationships
                .criteria
                .push(fixture.criterion.clone()),
            _ => fixture.draft.waypoints[0].relationships.executions.push(
                WaypointExecutionReference {
                    execution_id: "fixture-green".into(),
                    criterion: None,
                },
            ),
        }
        fixture.assert_rejected_unchanged();
    }
}

#[test]
fn existing_ticket_spec_links_resolve_without_explicit_waypoint_relationships() {
    let mut fixture = Fixture::new();
    fixture.promote();
    let saved = fixture.save().unwrap();
    let lifecycle = MissionLifecycle::open(fixture.root());
    let text = lifecycle.render_mission(saved.mission_id).unwrap();
    assert!(text.contains("Current Ticket"));
    assert!(text.contains("Current specification body"));
    assert!(
        lifecycle
            .get_mission_source(saved.mission_id)
            .unwrap()
            .waypoints[0]
            .relationships
            .is_empty()
    );
}

#[test]
fn native_import_publish_and_rootless_render_enforce_the_same_reference_contract() {
    let mut fixture = Fixture::new();
    fixture.link();
    let saved = fixture.save().unwrap();
    let store = MissionStore::open(fixture.root());
    let accepted = store.get(saved.mission_id).unwrap();
    let error = render(&accepted.bundle, accepted.mission_id).unwrap_err();
    assert_eq!(error[0].code, "mission-waypoint-link-root-required");
    let before = snapshot(&fixture.mission_dir());
    let mut invalid = accepted.bundle.clone();
    invalid.manifest.revision += 1;
    invalid.waypoints[0].relationships.criteria[0].criterion_id =
        CriterionId::new("dangling").unwrap();
    assert!(
        store
            .import(accepted.mission_id, invalid.clone(), saved.revision)
            .is_err()
    );
    assert!(
        store
            .publish(accepted.mission_id, invalid, saved.revision)
            .is_err()
    );
    assert_eq!(snapshot(&fixture.mission_dir()), before);
}

#[test]
fn promotion_rejects_conflicting_ticket_id_and_uri_without_writes() {
    let mut fixture = Fixture::new();
    fixture.promote();
    fixture.draft.waypoints[0].ownership_refs.ticket_uri =
        Some(format!("ce://default/ticket/{}", Uuid::new_v4()));
    fixture.assert_rejected_unchanged();
}

#[test]
fn malformed_entities_and_missing_criterion_content_are_not_silently_rendered() {
    for kind in ["ticket", "spec", "execution", "criteria"] {
        let mut fixture = Fixture::new();
        fixture.promote();
        fixture.link();
        let path = match kind {
            "ticket" => fixture
                .root()
                .join(".workflow-tools/ticket/tickets")
                .join(fixture.ticket.id.to_string())
                .join("ticket.toml"),
            "spec" | "criteria" => fixture
                .root()
                .join(".workflow-tools/spec/specs")
                .join(fixture.spec.id.to_string())
                .join("spec.toml"),
            _ => fixture
                .root()
                .join(".workflow-tools/test/executions/fixture-green.json"),
        };
        if kind == "criteria" {
            let mut spec = fixture.spec.clone();
            spec.extra.insert(
                "acceptance_criteria".into(),
                json!([{"id": fixture.criterion.criterion_id.as_str()}]),
            );
            fs::write(path, toml::to_string(&spec).unwrap()).unwrap();
        } else {
            fs::write(path, "not a domain manifest").unwrap();
        }
        fixture.assert_rejected_unchanged();
    }
}
