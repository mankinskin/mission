use super::*;

#[test]
fn native_migration_preserves_canonical_ownership_ids_uris_and_relationships() {
    for scope_only in [false, true] {
        let mut fixture = Fixture::new();
        fixture.promote();
        fixture.link();
        let saved = fixture.save().unwrap();
        let store = MissionStore::open(fixture.root());
        let mut bundle = store.get(saved.mission_id).unwrap().bundle;
        bundle.manifest.revision = 1;
        let child_id = Uuid::new_v4();
        let mut child = bundle.clone();
        child.waypoints[0].ticket_ref = None;
        child.waypoints[0].scope = Scope::SingleSession;
        child.waypoints[0].relationships = Default::default();
        store.import(child_id, child, 0).unwrap();
        let ticket_uri = format!("ce://default/ticket/{}", fixture.ticket.id);
        let child_uri = format!("ce://default/mission/{child_id}");
        bundle.waypoints[0].child_mission_ref = Some(child_uri.clone());
        if scope_only {
            bundle.waypoints[0].ticket_ref = None;
        }
        let id = Uuid::new_v4();
        store.import(id, bundle.clone(), 0).unwrap();
        let directory = fixture
            .root()
            .join(".workflow-tools/mission/missions")
            .join(id.to_string());
        assert!(!directory.join("source.json").exists());
        let history = fs::read(directory.join("history/revision-1.json")).unwrap();
        let ticket_before = snapshot(&fixture.root().join(".workflow-tools/ticket"));
        let lifecycle = MissionLifecycle::open(fixture.root());
        let mut journal =
            MissionLifecycle::plan_migration(&[fixture.root().to_path_buf()]).unwrap();
        MissionLifecycle::apply_migration(&mut journal).unwrap();
        assert_eq!(
            journal
                .records
                .iter()
                .find(|r| r.mission_id == id)
                .unwrap()
                .state,
            MigrationState::Applied
        );
        let source = lifecycle.get_mission_source(id).unwrap();
        let waypoint = &source.waypoints[0];
        assert_eq!(waypoint.ownership_refs.ticket_id, Some(fixture.ticket.id));
        assert_eq!(
            waypoint.ownership_refs.ticket_uri.as_deref(),
            Some(ticket_uri.as_str())
        );
        assert_eq!(waypoint.ownership_refs.child_mission_id, Some(child_id));
        assert_eq!(
            waypoint.ownership_refs.child_mission_uri.as_deref(),
            Some(child_uri.as_str())
        );
        assert_eq!(waypoint.relationships, bundle.waypoints[0].relationships);
        assert!(matches!(
            store.get(id).unwrap().bundle.waypoints[0].scope,
            Scope::TicketBacked { .. }
        ));
        assert_eq!(
            fs::read(directory.join("history/revision-1.json")).unwrap(),
            history
        );
        assert_eq!(
            snapshot(&fixture.root().join(".workflow-tools/ticket")),
            ticket_before
        );
        let mut edited = source.clone();
        edited.objective = "Save after native conversion".into();
        lifecycle.save_draft(id, edited, source.revision).unwrap();
        assert!(store.get(id).unwrap().bundle.waypoints[0].state.is_none());
    }
}

#[test]
fn native_migration_rejects_invalid_ownership_without_loss_or_source_writes() {
    for kind in [
        "ticket-malformed",
        "ticket-root",
        "ticket-nil",
        "ticket-conflict",
        "child-malformed",
        "child-root",
    ] {
        let mut fixture = Fixture::new();
        fixture.promote();
        fixture.link();
        let saved = fixture.save().unwrap();
        let store = MissionStore::open(fixture.root());
        let mut accepted = store.get(saved.mission_id).unwrap();
        let id = Uuid::new_v4();
        accepted.mission_id = id;
        accepted.bundle.manifest.revision = 1;
        let waypoint = &mut accepted.bundle.waypoints[0];
        match kind {
            "ticket-malformed" => waypoint.ticket_ref = Some("not-a-canonical-reference".into()),
            "ticket-root" => {
                waypoint.ticket_ref = Some(format!("ce://other/ticket/{}", fixture.ticket.id))
            }
            "ticket-nil" => {
                waypoint.ticket_ref = Some(format!("ce://default/ticket/{}", Uuid::nil()))
            }
            "ticket-conflict" => {
                waypoint.ticket_ref = Some(format!("ce://default/ticket/{}", Uuid::new_v4()))
            }
            "child-malformed" => {
                waypoint.child_mission_ref = Some("not-a-canonical-reference".into())
            }
            _ => {
                waypoint.child_mission_ref = Some(format!("ce://other/mission/{}", Uuid::new_v4()))
            }
        }
        // A pre-existing native record must be inspected without repairing it.
        let directory = fixture
            .root()
            .join(".workflow-tools/mission/missions")
            .join(id.to_string());
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            directory.join("accepted.json"),
            serde_json::to_vec_pretty(&accepted).unwrap(),
        )
        .unwrap();
        let before = snapshot(&directory);
        assert!(
            MissionLifecycle::plan_migration(&[fixture.root().to_path_buf()]).is_err(),
            "{kind}"
        );
        assert_eq!(snapshot(&directory), before, "{kind}");
    }
}
