use super::*;

pub(crate) fn bundle() -> MissionBundle {
    MissionBundle {
        manifest: BundleManifest {
            schema_version: SCHEMA_VERSION,
            revision: 1,
            title: "Deterministic fixture".into(),
            objective: "Verify stable event metadata in isolated stores.".into(),
            waypoint_files: vec![],
            execution_order: vec!["verify".into()],
            artifacts: vec![],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
            operator_results: vec![],
        },
        waypoints: vec![WaypointFragment {
            outcome_plan: None,
            waypoint_id: "verify".into(),
            title: "Verify".into(),
            depends_on: vec![],
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            child_mission_ref: None,
            relationships: WaypointRelationships::default(),
            session_package: "representative-mission-fixtures".into(),
            prompt: "Verify deterministic test metadata.".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "cargo test".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "test(mission-api): verify deterministic fixtures".into(),
            },
            part_markdown: "Synthetic fixture Part.".into(),
        }],
    }
}

pub(crate) fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(path).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}
pub(crate) fn generate_event_tree(
    workspace: &Path,
    mission_id: Uuid,
    event_id: Uuid,
) -> BTreeMap<String, Vec<u8>> {
    let store = MissionStore::init(workspace)
        .unwrap()
        .with_test_event_metadata(event_id, 1_798_000_000_000);
    store.import(mission_id, bundle(), 0).unwrap();
    let binding = store.output_contract_binding(mission_id).unwrap();
    let request = serde_json::json!({
        "revision": 1,
        "decision": "approve",
        "signature": binding,
        "actor": "synthetic reviewer",
        "actor_source": "fixture-authored",
        "metadata": {"claim": "not human approval"}
    });
    store
        .record_approval_event_json(mission_id, request.to_string().as_bytes())
        .unwrap();
    store.verify_output_history(mission_id).unwrap();
    let mission_directory = workspace
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string());
    tree(&mission_directory)
}

#[derive(Default)]
struct ReferenceInventory {
    specification_ids: BTreeSet<Uuid>,
    criteria: BTreeMap<Uuid, BTreeSet<String>>,
    ticket_ids: BTreeSet<Uuid>,
    execution_ids: BTreeSet<String>,
}

impl ReferenceInventory {
    fn add_relationships(&mut self, links: &WaypointRelationships) {
        self.specification_ids
            .extend(links.specification_ids.iter().copied());
        for criterion in &links.criteria {
            self.specification_ids.insert(criterion.specification_id);
            self.criteria
                .entry(criterion.specification_id)
                .or_default()
                .insert(criterion.criterion_id.as_str().into());
        }
        self.execution_ids.extend(
            links
                .executions
                .iter()
                .map(|execution| execution.execution_id.clone()),
        );
    }

    fn add_ticket(&mut self, reference: Option<&str>) {
        if let Some(id) = reference
            .and_then(|value| value.strip_prefix("ce://"))
            .and_then(|value| value.split('/').nth(2))
            .and_then(|value| Uuid::parse_str(value).ok())
        {
            self.ticket_ids.insert(id);
        }
    }
}

fn references_for_bundle(bundle: &MissionBundle) -> ReferenceInventory {
    let mut references = ReferenceInventory::default();
    for waypoint in &bundle.waypoints {
        references.add_relationships(&waypoint.relationships);
        references.add_ticket(waypoint.ticket_ref.as_deref());
        if let Scope::TicketBacked { ticket_ref } = &waypoint.scope {
            references.add_ticket(Some(ticket_ref));
        }
    }
    references
}

fn references_for_draft(draft: &TypedDraft) -> ReferenceInventory {
    let mut references = ReferenceInventory::default();
    for waypoint in &draft.waypoints {
        references.add_relationships(&waypoint.relationships);
        references.add_ticket(waypoint.ownership_refs.ticket_uri.as_deref());
        if let Some(id) = waypoint.ownership_refs.ticket_id {
            references.ticket_ids.insert(id);
        }
    }
    references
}

fn seed_reference_stores(root: &Path, references: &ReferenceInventory) {
    let mut spec_store = SpecStore::init(&root.join(".workflow-tools/spec")).unwrap();
    for id in &references.specification_ids {
        let criteria = references
            .criteria
            .get(id)
            .into_iter()
            .flatten()
            .map(|criterion_id| AcceptanceCriterion {
                id: criterion_id.clone(),
                statement: format!(
                    "Synthetic criterion {criterion_id} for isolated fixture review."
                ),
                expected_property_ids: vec![],
                required_evidence_ids: vec![],
            })
            .collect();
        let mut specification = SpecManifest::new(
            &format!("representative-mission/{id}"),
            "Synthetic representative Mission specification",
            "mission",
        );
        specification.id = *id;
        specification.set_state("accepted");
        specification.set_acceptance_criteria(criteria);
        spec_store
            .create(
                &specification,
                "Synthetic local fixture record. Not a requester-approved specification.",
                None,
            )
            .unwrap();
    }

    for id in &references.ticket_ids {
        let related_specs: Vec<_> = references
            .specification_ids
            .iter()
            .map(|spec_id| {
                json!({
                    "spec_id": spec_id,
                    "workspace": "default",
                    "store_root": ".workflow-tools/spec"
                })
            })
            .collect();
        let ticket: TicketManifest = serde_json::from_value(json!({
            "id": id,
            "created_at": "2026-10-29T00:00:00Z",
            "title": "Synthetic representative Mission Ticket",
            "state": "done",
            "related_specs": related_specs
        }))
        .unwrap();
        TicketFs::create(&ticket, &root.join(".workflow-tools/ticket/tickets"), None).unwrap();
    }

    if !references.execution_ids.is_empty() {
        let store = TestStoreConfig::new(root.join(".workflow-tools/test"));
        let spec_ids: Vec<_> = references
            .specification_ids
            .iter()
            .map(Uuid::to_string)
            .collect();
        let criterion_ids: Vec<_> = references.criteria.values().flatten().cloned().collect();
        for id in &references.execution_ids {
            let execution: ValidationExecution = serde_json::from_value(json!({
                "id": id,
                "validation_spec_id": "representative-mission-synthetic-check",
                "outcome": "passed",
                "executed_at": "2026-10-29T00:00:00Z",
                "detail": "Synthetic fixture evidence; no product validation is claimed.",
                "links": {
                    "spec_ids": spec_ids,
                    "acceptance_criterion_ids": criterion_ids
                },
                "provenance": {
                    "domain": "mission",
                    "operation": "representative-fixture",
                    "run_id": "synthetic-only"
                }
            }))
            .unwrap();
            store.record_execution(&execution).unwrap();
        }
    }
}

pub(crate) fn initial_inputs() -> MissionOutputInputs {
    MissionOutputInputs {
        merged_request: Some("Synthetic request state 1; not source execution or approval.".into()),
        continuations: BTreeMap::new(),
    }
}

pub(crate) fn fixture_paths() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/representative-missions")
}

pub(crate) fn fixture_cases() -> [(&'static str, &'static str); 3] {
    [
        (
            "documentation-workflow",
            "documentation-workflow.native-v2.json",
        ),
        ("mission-integration", "mission-integration.typed-v3.json"),
        ("ticket-reliability", "ticket-reliability.native-v2.json"),
    ]
}

pub(crate) fn generate_fixture(workspace: &Path, authoring: &Path) -> Uuid {
    let authoring_bytes = fs::read(authoring).unwrap();
    let store = MissionStore::init(workspace)
        .unwrap()
        .with_test_event_metadata(
            Uuid::from_u128(0x0000000000004000800000000000f00d),
            1_798_000_000_000,
        );
    if authoring.extension().and_then(|ext| ext.to_str()) == Some("json")
        && authoring
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".native-v2.json"))
    {
        let input: NativeFixtureInput = serde_json::from_slice(&authoring_bytes).unwrap();
        seed_reference_stores(workspace, &references_for_bundle(&input.bundle));
        let mission_id = input.mission_id;
        let expected_bundle = input.bundle;
        let published = store
            .publish_with_inputs(
                mission_id,
                expected_bundle.clone(),
                0,
                &initial_inputs(),
                None,
            )
            .unwrap();
        assert_eq!(published.bundle, expected_bundle);
        mission_id
    } else {
        let draft: TypedDraft = serde_json::from_slice(&authoring_bytes).unwrap();
        seed_reference_stores(workspace, &references_for_draft(&draft));
        let mission_id = draft.mission_id;
        let lifecycle = MissionLifecycle::open(workspace);
        let published_source = lifecycle.publish_draft(mission_id, draft, 0).unwrap();
        assert_eq!(
            lifecycle.get_mission_source(mission_id).unwrap(),
            published_source
        );
        let accepted = store.get(mission_id).unwrap();
        for waypoint in &published_source.waypoints {
            assert_eq!(
                accepted
                    .bundle
                    .waypoints
                    .iter()
                    .find(|accepted| accepted.waypoint_id == waypoint.waypoint_id)
                    .unwrap()
                    .outcome_plan,
                waypoint.outcome_plan
            );
        }

        let first = store.get(mission_id).unwrap();
        let binding = store.output_contract_binding(mission_id).unwrap();
        let reaction = serde_json::json!({
            "revision": 1,
            "decision": "changes-requested",
            "signature": binding,
            "actor": "synthetic fixture reviewer",
            "actor_source": "fixture-authored",
            "metadata": {"notice": "not authentication or human approval"}
        });
        store
            .record_approval_event_json(mission_id, reaction.to_string().as_bytes())
            .unwrap();

        let mut revision_two = first.bundle;
        revision_two.manifest.revision = 2;
        revision_two.manifest.objective.push_str(
            " Revision 2 preserves the source scope while adding synthetic fixture context.",
        );
        let initial_inputs = MissionOutputInputs {
            merged_request: Some("Synthetic REQUEST state 2.".into()),
            continuations: BTreeMap::from([(
                "reviewer/initial.md".into(),
                "Synthetic continuation retained in later whole states.".into(),
            )]),
        };
        store
            .publish_with_inputs(mission_id, revision_two, 1, &initial_inputs, None)
            .unwrap();

        let current = store.get(mission_id).unwrap();
        let mut revision_three = current.bundle;
        revision_three.manifest.revision = 3;
        revision_three.manifest.objective.push_str(
            " Revision 3 records a changed synthetic request without replacing prior continuation.",
        );
        let changed_inputs = MissionOutputInputs {
            merged_request: Some("Changed synthetic REQUEST state 3.".into()),
            continuations: BTreeMap::from([(
                "reviewer/follow-up.md".into(),
                "A separate fixture-authored continuation for the later whole state.".into(),
            )]),
        };
        store
            .publish_with_inputs(mission_id, revision_three, 2, &changed_inputs, None)
            .unwrap();
        mission_id
    }
}

pub(crate) fn generated_directory(workspace: &Path, mission_id: Uuid) -> PathBuf {
    workspace
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string())
}

pub(crate) fn assert_diagnostic(error: MissionError, expected_code: &str) {
    let diagnostics = error
        .diagnostics()
        .expect("rejected fixture operation should return diagnostics");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == expected_code),
        "expected diagnostic {expected_code}, got {diagnostics:?}"
    );
}

pub(crate) fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for (relative, bytes) in tree(source) {
        let path = destination.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }
}
