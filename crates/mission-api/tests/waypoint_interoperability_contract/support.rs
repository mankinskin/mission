use super::*;

pub(crate) struct Fixture {
    pub(crate) workspace: tempfile::TempDir,
    pub(crate) draft: TypedDraft,
    pub(crate) spec: SpecManifest,
    pub(crate) criterion: CriterionReference,
    pub(crate) ticket: TicketManifest,
}

impl Fixture {
    pub(crate) fn new() -> Self {
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

    pub(crate) fn root(&self) -> &Path {
        self.workspace.path()
    }

    pub(crate) fn promote(&mut self) {
        self.draft.waypoints[0].ownership_refs.ticket_id = Some(self.ticket.id);
    }

    pub(crate) fn link(&mut self) {
        self.draft.waypoints[0].relationships = WaypointRelationships {
            specification_ids: vec![self.spec.id],
            criteria: vec![self.criterion.clone()],
            executions: vec![WaypointExecutionReference {
                execution_id: "fixture-green".into(),
                criterion: None,
            }],
        };
    }

    pub(crate) fn save(&self) -> Result<TypedDraft, MissionError> {
        MissionLifecycle::open(self.root()).save_draft(
            self.draft.mission_id,
            self.draft.clone(),
            self.draft.revision,
        )
    }

    pub(crate) fn mission_dir(&self) -> PathBuf {
        self.root()
            .join(".workflow-tools/mission/missions")
            .join(self.draft.mission_id.to_string())
    }

    pub(crate) fn assert_rejected_unchanged(&self) {
        let before = snapshot(&self.mission_dir());
        let error = self.save().unwrap_err();
        assert!(matches!(error, MissionError::Validation(..)), "{error:?}");
        assert_eq!(snapshot(&self.mission_dir()), before);
    }
}

pub(crate) fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    pub(crate) fn walk(root: &Path, path: &Path, values: &mut BTreeMap<PathBuf, Vec<u8>>) {
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
