use mission_api::*;
use serde_json::json;
use spec_api::{AcceptanceCriterion, SpecManifest, SpecStore};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use test_api::{TestStoreConfig, ValidationExecution};
use ticket_api::{model::ticket::TicketManifest, storage::ticket_fs::TicketFs};
use uuid::Uuid;

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

struct Fixture {
    workspace: tempfile::TempDir,
    draft: TypedDraft,
    spec: SpecManifest,
    criterion: CriterionReference,
    ticket: TicketManifest,
}

impl Fixture {
    fn new() -> Self {
        let workspace = tempfile::Builder::new()
            .prefix(".w4-fixture-")
            .tempdir_in(env!("CARGO_MANIFEST_DIR"))
            .unwrap();
        let root = workspace.path();
        let mut spec = SpecManifest::new(
            "mission/relationships",
            "Canonical specification",
            "mission",
        );
        spec.set_acceptance_criteria(vec![AcceptanceCriterion {
            id: "mission-waypoint-link-fixture".into(),
            statement: "Current canonical criterion".into(),
            expected_property_ids: vec![],
            required_evidence_ids: vec![],
        }]);
        SpecStore::init(&root.join(".workflow-tools/spec"))
            .unwrap()
            .create(&spec, "Current specification body", None)
            .unwrap();
        let criterion = CriterionReference {
            specification_id: spec.id,
            criterion_id: CriterionId::new("mission-waypoint-link-fixture").unwrap(),
        };
        let ticket: TicketManifest = serde_json::from_value(json!({
            "id": Uuid::new_v4(),
            "created_at": "2026-10-29T00:00:00Z",
            "title": "Current Ticket",
            "state": "done",
            "related_specs": [{
                "spec_id": spec.id,
                "workspace": "default",
                "store_root": ".workflow-tools/spec"
            }]
        }))
        .unwrap();
        TicketFs::create(&ticket, &root.join(".workflow-tools/ticket/tickets"), None).unwrap();
        let execution: ValidationExecution = serde_json::from_value(json!({
            "id": "fixture-green",
            "validation_spec_id": "fixture-validation",
            "outcome": "passed",
            "executed_at": "2026-10-29T00:00:00Z",
            "detail": "Current execution detail",
            "links": {"spec_ids": [spec.id], "acceptance_criterion_ids": [criterion.criterion_id.as_str()]},
            "provenance": {"domain": "mission", "operation": "waypoint-links", "run_id": "fixture"}
        })).unwrap();
        TestStoreConfig::new(root.join(".workflow-tools/test"))
            .record_execution(&execution)
            .unwrap();
        let lifecycle = MissionLifecycle::open(root);
        let mut draft = lifecycle
            .create_draft("Relationships".into(), "Keep authorities separate".into())
            .unwrap();
        draft.waypoints.push(TypedWaypoint {
            waypoint_id: "verify".into(),
            title: "Verify".into(),
            prompt: "Check current entities".into(),
            part: PartSource {
                waypoint_id: "verify".into(),
                document: Document {
                    blocks: vec![Block::Paragraph {
                        inlines: vec![Inline::Text {
                            text: "Independent authored Part".into(),
                        }],
                    }],
                },
            },
            validation: vec![ValidationGate {
                command: "true".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "Verify relationships".into(),
            },
            ..Default::default()
        });
        Self {
            workspace,
            draft,
            spec,
            criterion,
            ticket,
        }
    }

    fn root(&self) -> &Path {
        self.workspace.path()
    }

    fn promote(&mut self) {
        self.draft.waypoints[0].ownership_refs.ticket_id = Some(self.ticket.id);
    }

    fn link(&mut self) {
        self.draft.waypoints[0].relationships = WaypointRelationships {
            specification_ids: vec![self.spec.id],
            criteria: vec![self.criterion.clone()],
            executions: vec![WaypointExecutionReference {
                execution_id: "fixture-green".into(),
                criterion: None,
            }],
        };
    }

    fn save(&self) -> Result<TypedDraft, MissionError> {
        MissionLifecycle::open(self.root()).save_draft(
            self.draft.mission_id,
            self.draft.clone(),
            self.draft.revision,
        )
    }

    fn mission_dir(&self) -> PathBuf {
        self.root()
            .join(".workflow-tools/mission/missions")
            .join(self.draft.mission_id.to_string())
    }

    fn assert_rejected_unchanged(&self) {
        let before = snapshot(&self.mission_dir());
        let error = self.save().unwrap_err();
        assert!(matches!(error, MissionError::Validation(..)), "{error:?}");
        assert_eq!(snapshot(&self.mission_dir()), before);
    }
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, values: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, values);
            } else if path.file_name().unwrap() != ".ticket-lock" {
                values.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut values = BTreeMap::new();
    walk(root, root, &mut values);
    values
}

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
    assert!(
        roadmap.contains(
            &fixture
                .root()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .to_string()
        )
    );
    assert!(
        String::from_utf8(rendered.parts["waypoints/01-verify.md"].clone())
            .unwrap()
            .contains("Updated Ticket")
    );
    assert!(
        lifecycle
            .render_mission(saved.mission_id)
            .unwrap()
            .contains("Updated Ticket")
    );
    assert_eq!(snapshot(&fixture.mission_dir()), before);
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
