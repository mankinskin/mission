use super::*;

pub(crate) fn plan(name: &str) -> WaypointOutcomePlan {
    WaypointOutcomePlan {
        expected_outcome: format!("{name} is ready"),
        alternative_outcomes: vec![AlternativeOutcome {
            outcome: format!("{name} is blocked"),
            acceptable_handling: vec!["Inspect the failure".into()],
        }],
    }
}
pub(crate) fn legacy(id: &str) -> TypedWaypoint {
    TypedWaypoint {
        outcome_plan: None,
        waypoint_id: id.into(),
        title: format!("{id} title"),
        prompt: format!("{id} prompt"),
        part: PartSource {
            waypoint_id: id.into(),
            document: Document {
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text {
                        text: format!("{id} body"),
                    }],
                }],
            },
        },
        validation: vec![ValidationGate {
            command: "cargo test".into(),
            cwd: None,
        }],
        commit_checkpoint: CommitCheckpoint {
            message: format!("{id} checkpoint"),
        },
        non_goals: vec![format!("{id} non-goal")],
        ..Default::default()
    }
}
pub(crate) fn waypoint(id: &str) -> TypedWaypoint {
    TypedWaypoint {
        outcome_plan: Some(plan(id)),
        ..legacy(id)
    }
}
pub(crate) struct Fixture {
    dir: TempDir,
    pub(crate) lifecycle: MissionLifecycle,
    pub(super) id: Uuid,
}
impl Fixture {
    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }
    pub(crate) fn mission_dir(&self) -> PathBuf {
        self.lifecycle.mission_directory(self.id)
    }
}
pub(crate) fn seeded(schema: u32, ids: &[&str]) -> Fixture {
    let dir = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(dir.path());
    let mut draft = lifecycle
        .create_draft("Plan".into(), "Goal".into())
        .unwrap();
    let id = draft.mission_id;
    draft.schema_version = schema;
    draft.waypoints = ids
        .iter()
        .map(|name| {
            if schema == 3 {
                waypoint(name)
            } else {
                legacy(name)
            }
        })
        .collect();
    lifecycle.publish_draft(id, draft, 1).unwrap();
    Fixture { dir, lifecycle, id }
}
pub(crate) fn apply(
    f: &Fixture,
    request_id: &str,
    expected_revision: u64,
    operation: PlanOperation,
) -> Result<PlanOperationOutcome, MissionError> {
    f.lifecycle.apply_plan_operation(
        f.id,
        PlanOperationRequest {
            request_id: request_id.into(),
            expected_revision,
            operation,
        },
    )
}
pub(crate) fn code(error: &MissionError) -> String {
    match error {
        MissionError::StaleRevision { .. } => "revision_conflict".into(),
        MissionError::PublicationRecovery { .. } => "publication_recovery".into(),
        MissionError::Validation(_, diagnostics) => diagnostics[0].code.to_string(),
        other => panic!("unexpected error {other:?}"),
    }
}
pub(crate) fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    pub(crate) fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
pub(crate) fn rejected(
    f: &Fixture,
    request_id: &str,
    expected: u64,
    operation: PlanOperation,
    wanted: &str,
) {
    let before = snapshot(f.root());
    let error = apply(f, request_id, expected, operation).unwrap_err();
    assert_eq!(code(&error), wanted, "{error:?}");
    assert_eq!(
        snapshot(f.root()),
        before,
        "{wanted}: source, projection and journal bytes must be preserved"
    );
}
pub(crate) fn review(f: &Fixture) -> Value {
    serde_json::to_value(f.lifecycle.review_plan(f.id).unwrap()).unwrap()
}
pub(crate) fn ids(review: &Value) -> Vec<String> {
    review["waypoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["waypoint_id"].as_str().unwrap().to_owned())
        .collect()
}
pub(crate) fn source_ids(f: &Fixture) -> Vec<String> {
    f.lifecycle
        .get_mission_source(f.id)
        .unwrap()
        .waypoints
        .into_iter()
        .map(|w| w.waypoint_id)
        .collect()
}
pub(crate) fn projection(f: &Fixture) -> Value {
    serde_json::from_slice(&fs::read(f.mission_dir().join("review-plan.json")).unwrap()).unwrap()
}
pub(crate) fn evidence(id: &str) -> OperatorResult {
    OperatorResult {
        waypoint_id: id.into(),
        outcome: "passed".into(),
        operator_command: "cargo test".into(),
        evidence_pointer: format!("ce://default/test/receipt-{id}"),
    }
}
