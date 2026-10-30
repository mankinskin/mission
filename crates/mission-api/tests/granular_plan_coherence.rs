use mission_api::*;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tempfile::{TempDir, tempdir};
use uuid::Uuid;

fn legacy_free_waypoint(id: &str) -> TypedWaypoint {
    TypedWaypoint {
        outcome_plan: Some(WaypointOutcomePlan {
            expected_outcome: format!("{id} is ready"),
            alternative_outcomes: vec![AlternativeOutcome {
                outcome: format!("{id} is blocked"),
                acceptable_handling: vec!["Inspect the failure".into()],
            }],
        }),
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
struct Fixture {
    dir: TempDir,
    lifecycle: MissionLifecycle,
    id: Uuid,
}
impl Fixture {
    fn mission_dir(&self) -> PathBuf {
        self.lifecycle.mission_directory(self.id)
    }
}
fn seeded(ids: &[&str]) -> Fixture {
    let dir = tempdir().unwrap();
    let lifecycle = MissionLifecycle::open(dir.path());
    let mut draft = lifecycle
        .create_draft("Plan".into(), "Goal".into())
        .unwrap();
    let id = draft.mission_id;
    draft.schema_version = 3;
    draft.waypoints = ids.iter().map(|name| legacy_free_waypoint(name)).collect();
    lifecycle.publish_draft(id, draft, 1).unwrap();
    Fixture { dir, lifecycle, id }
}
fn apply(
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
fn code(error: &MissionError) -> String {
    match error {
        MissionError::StaleRevision { .. } => "revision_conflict".into(),
        MissionError::Validation(_, diagnostics) => diagnostics[0].code.to_string(),
        other => panic!("unexpected error {other:?}"),
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
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
fn review(f: &Fixture) -> Value {
    serde_json::to_value(f.lifecycle.review_plan(f.id).unwrap()).unwrap()
}
fn projection(f: &Fixture) -> Value {
    serde_json::from_slice(&fs::read(f.mission_dir().join("review-plan.json")).unwrap()).unwrap()
}
fn requirement(id: &str, deferred: bool) -> Requirement {
    Requirement {
        id: id.into(),
        text: format!("{id} text"),
        deferred,
        canonical_criteria: vec![],
    }
}
fn own(id: &str, owners: &[&str]) -> PlanRequirement {
    PlanRequirement {
        requirement: requirement(id, false),
        owner_waypoint_ids: owners.iter().map(|o| (*o).to_owned()).collect(),
    }
}

#[test]
fn review_plan_exposes_prompt_part_checkpoint_and_never_writes() {
    let f = seeded(&["a", "b"]);
    let before = snapshot(f.dir.path());
    let plan = review(&f);
    assert_eq!(snapshot(f.dir.path()), before, "review_plan is read-only");
    let first = &plan["waypoints"][0];
    assert_eq!(first["prompt"], "a prompt");
    assert_eq!(first["commit_checkpoint"]["message"], "a checkpoint");
    assert_eq!(
        first["part"],
        serde_json::to_value(&legacy_free_waypoint("a").part.document).unwrap()
    );
    assert_eq!(plan["formal_review"], "not-recorded");
    assert_eq!(first["validation_status"], "declared");
}

#[test]
fn same_order_semantic_edit_changes_identity_and_concurrent_reads_stay_coherent() {
    let f = seeded(&["a", "b"]);
    let first = review(&f);
    let mut edited = legacy_free_waypoint("a");
    edited.prompt = "edited prompt".into();
    apply(
        &f,
        "edit",
        2,
        PlanOperation::UpdateWaypoint { waypoint: edited },
    )
    .unwrap();
    let second = review(&f);
    assert_eq!(
        first["waypoints"][0]["waypoint_id"],
        second["waypoints"][0]["waypoint_id"]
    );
    assert_ne!(first["plan_digest"], second["plan_digest"]);
    assert_eq!(second["waypoints"][0]["prompt"], "edited prompt");
    let mut known: BTreeMap<u64, String> = BTreeMap::from([
        (2, first["plan_digest"].as_str().unwrap().to_owned()),
        (3, second["plan_digest"].as_str().unwrap().to_owned()),
    ]);
    let observed = Mutex::new(Vec::new());
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                let plan = f.lifecycle.review_plan(f.id).expect("coherent snapshot");
                observed
                    .lock()
                    .unwrap()
                    .push((plan.revision, plan.plan_digest));
            }
        });
        for n in 0..12u64 {
            let outcome = apply(
                &f,
                &format!("c{n}"),
                3 + n,
                PlanOperation::SetGoal {
                    title: "Plan".into(),
                    objective: format!("goal {n}"),
                },
            )
            .unwrap();
            known.insert(outcome.revision, outcome.plan_digest);
        }
        done.store(true, Ordering::SeqCst);
    });
    for (revision, digest) in observed.into_inner().unwrap() {
        assert_eq!(
            known[&revision], digest,
            "mixed-revision read at {revision}"
        );
    }
}

#[test]
fn replacement_inherits_requirement_ownership_and_owner_removal_never_orphans() {
    let f = seeded(&["a", "b", "c"]);
    apply(
        &f,
        "q",
        2,
        PlanOperation::SetRequirements {
            requirements: vec![own("req-one", &["b"]), own("req-two", &["c"])],
        },
    )
    .unwrap();
    apply(
        &f,
        "rm",
        3,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: Some(legacy_free_waypoint("b2")),
        },
    )
    .unwrap();
    let plan = review(&f);
    assert_eq!(plan["waypoints"][1]["waypoint_id"], "b2");
    assert_eq!(plan["waypoints"][1]["requirement_ids"], json!(["req-one"]));
    let before = snapshot(f.dir.path());
    let error = apply(
        &f,
        "rm-owner",
        4,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "c".into(),
            replacement: None,
        },
    )
    .unwrap_err();
    assert_eq!(code(&error), "unowned-requirement");
    assert_eq!(snapshot(f.dir.path()), before);
}

#[test]
fn evidence_stays_listed_but_not_current_after_a_semantic_edit() {
    let f = seeded(&["a", "b"]);
    let first = review(&f);
    let digest = first["plan_digest"].as_str().unwrap().to_owned();
    apply(
        &f,
        "ev",
        2,
        PlanOperation::RecordEvidence {
            plan_digest: digest.clone(),
            result: OperatorResult {
                waypoint_id: "a".into(),
                outcome: "passed".into(),
                operator_command: "cargo test".into(),
                evidence_pointer: "ce://default/test/receipt-a".into(),
            },
        },
    )
    .unwrap();
    let evidenced = review(&f);
    assert_eq!(evidenced["waypoints"][0]["evidence"][0]["current"], true);
    let mut edited = legacy_free_waypoint("b");
    edited.prompt = "semantic edit".into();
    apply(
        &f,
        "edit",
        3,
        PlanOperation::UpdateWaypoint { waypoint: edited },
    )
    .unwrap();
    let after = review(&f);
    let item = &after["waypoints"][0]["evidence"][0];
    assert_eq!(item["current"], false);
    assert_eq!(item["bound_plan_digest"], digest);
    assert_eq!(after["waypoints"][0]["review_status"], "pending");
    assert_eq!(after["evidence_identity"], evidenced["evidence_identity"]);
    assert_ne!(after["plan_digest"].as_str().unwrap(), digest);
    assert_eq!(after["formal_review"], "not-recorded");
    assert_eq!(projection(&f), after);
}

#[test]
fn plan_operations_retain_earlier_inputs_provenance_and_history_bytes() {
    let f = seeded(&["a"]);
    let dir = f.mission_dir();
    fs::write(dir.join("intake-receipt.json"), b"{\"w2\":true}\n").unwrap();
    let before = snapshot(&dir);
    apply(
        &f,
        "keep",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: legacy_free_waypoint("b"),
            after: None,
        },
    )
    .unwrap();
    let after = snapshot(&dir);
    for (path, bytes) in &before {
        let name = path.to_string_lossy();
        let immutable = name == "intake-receipt.json"
            || (name.starts_with("history/") && !name.starts_with("history/states/"));
        if immutable {
            assert_eq!(
                after.get(path),
                Some(bytes),
                "{name} must be retained verbatim"
            );
        }
    }
    assert!(after.contains_key(Path::new("history/provenance-revision-3.json")));
    assert!(after.contains_key(Path::new("history/revision-2.json")));
}

#[test]
fn repair_republishes_a_review_plan_matching_its_own_fingerprint() {
    let f = seeded(&["a"]);
    apply(
        &f,
        "goal",
        2,
        PlanOperation::SetGoal {
            title: "Plan".into(),
            objective: "Changed".into(),
        },
    )
    .unwrap();
    let source_path = f.mission_dir().join("source.json");
    let mut source: Value = serde_json::from_slice(&fs::read(&source_path).unwrap()).unwrap();
    source["revision"] = json!(2);
    fs::write(&source_path, serde_json::to_vec_pretty(&source).unwrap()).unwrap();
    let root = f.dir.path();
    let mut request = RepairRequest {
        source_root: root.to_path_buf(),
        policy: REPAIR_POLICY.into(),
        records: vec![RepairRecordRequest {
            mission_id: f.id,
            expected_revision: 3,
            accepted_sha256: String::new(),
            source_sha256: String::new(),
            payload_fingerprint: String::new(),
        }],
    };
    let discovery = f.lifecycle.repair_preview(root, request.clone()).unwrap();
    let observed = &discovery.records[0];
    request.records[0].accepted_sha256 = observed.observed_accepted_sha256.clone().unwrap();
    request.records[0].source_sha256 = observed.observed_source_sha256.clone().unwrap();
    request.records[0].payload_fingerprint = observed.payload_fingerprint.clone().unwrap();
    let preview = f.lifecycle.repair_preview(root, request.clone()).unwrap();
    assert_eq!(preview.status, "ready", "{preview:?}");
    let applied = f
        .lifecycle
        .repair_apply(
            root,
            request,
            preview.planned_journal_identity.as_deref().unwrap(),
        )
        .unwrap();
    assert_eq!(applied.status, "applied", "{applied:?}");
    let plan = review(&f);
    assert_eq!(
        projection(&f),
        plan,
        "repair must publish the matching plan"
    );
    assert_eq!(plan["objective"], "Changed");
}
