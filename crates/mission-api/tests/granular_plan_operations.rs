use mission_api::*;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::{TempDir, tempdir};
use uuid::Uuid;

fn plan(name: &str) -> WaypointOutcomePlan {
    WaypointOutcomePlan {
        expected_outcome: format!("{name} is ready"),
        alternative_outcomes: vec![AlternativeOutcome {
            outcome: format!("{name} is blocked"),
            acceptable_handling: vec!["Inspect the failure".into()],
        }],
    }
}
fn legacy(id: &str) -> TypedWaypoint {
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
fn waypoint(id: &str) -> TypedWaypoint {
    TypedWaypoint {
        outcome_plan: Some(plan(id)),
        ..legacy(id)
    }
}
struct Fixture {
    dir: TempDir,
    lifecycle: MissionLifecycle,
    id: Uuid,
}
impl Fixture {
    fn root(&self) -> &Path {
        self.dir.path()
    }
    fn mission_dir(&self) -> PathBuf {
        self.lifecycle.mission_directory(self.id)
    }
}
fn seeded(schema: u32, ids: &[&str]) -> Fixture {
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
        MissionError::PublicationRecovery { .. } => "publication_recovery".into(),
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
fn rejected(f: &Fixture, request_id: &str, expected: u64, operation: PlanOperation, wanted: &str) {
    let before = snapshot(f.root());
    let error = apply(f, request_id, expected, operation).unwrap_err();
    assert_eq!(code(&error), wanted, "{error:?}");
    assert_eq!(
        snapshot(f.root()),
        before,
        "{wanted}: source, projection and journal bytes must be preserved"
    );
}
fn review(f: &Fixture) -> Value {
    serde_json::to_value(f.lifecycle.review_plan(f.id).unwrap()).unwrap()
}
fn ids(review: &Value) -> Vec<String> {
    review["waypoints"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["waypoint_id"].as_str().unwrap().to_owned())
        .collect()
}
fn source_ids(f: &Fixture) -> Vec<String> {
    f.lifecycle
        .get_mission_source(f.id)
        .unwrap()
        .waypoints
        .into_iter()
        .map(|w| w.waypoint_id)
        .collect()
}
fn projection(f: &Fixture) -> Value {
    serde_json::from_slice(&fs::read(f.mission_dir().join("review-plan.json")).unwrap()).unwrap()
}
fn evidence(id: &str) -> OperatorResult {
    OperatorResult {
        waypoint_id: id.into(),
        outcome: "passed".into(),
        operator_command: "cargo test".into(),
        evidence_pointer: format!("ce://default/test/receipt-{id}"),
    }
}

#[test]
fn stale_revision_and_unknown_identity_preserve_authority_and_projection_bytes() {
    let f = seeded(3, &["a", "b", "c"]);
    rejected(
        &f,
        "r1",
        1,
        PlanOperation::SetGoal {
            title: "T".into(),
            objective: "O".into(),
        },
        "revision_conflict",
    );
    rejected(
        &f,
        "r2",
        2,
        PlanOperation::UpdateWaypoint {
            waypoint: waypoint("ghost"),
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "r3",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "ghost".into(),
            replacement: None,
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "r4",
        2,
        PlanOperation::ReorderWaypoints {
            order: vec!["c".into(), "b".into(), "ghost".into()],
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "r5",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: waypoint("d"),
            after: Some("ghost".into()),
        },
        "waypoint_not_found",
    );
}

#[test]
fn request_id_replays_without_a_new_revision_and_rejects_reuse_with_different_content() {
    let f = seeded(3, &["a"]);
    let create = || PlanOperation::CreateWaypoint {
        waypoint: waypoint("b"),
        after: None,
    };
    let first = apply(&f, "req-1", 2, create()).unwrap();
    assert_eq!((first.revision, first.replayed), (3, false));
    let before = snapshot(f.root());
    let replay = apply(&f, "req-1", 2, create()).unwrap();
    assert!(replay.replayed);
    assert_eq!(
        (replay.revision, &replay.plan_digest),
        (3, &first.plan_digest)
    );
    assert_eq!(snapshot(f.root()), before);
    rejected(
        &f,
        "req-1",
        2,
        PlanOperation::SetGoal {
            title: "T".into(),
            objective: "different".into(),
        },
        "request_id_conflict",
    );
    apply(
        &f,
        "req-2",
        3,
        PlanOperation::SetGoal {
            title: "T".into(),
            objective: "later".into(),
        },
    )
    .unwrap();
    let later = snapshot(f.root());
    assert!(apply(&f, "req-1", 2, create()).unwrap().replayed);
    assert_eq!(snapshot(f.root()), later);
}

#[test]
fn goal_create_update_and_reorder_publish_coherent_revisions_and_total_order() {
    let f = seeded(3, &["a", "b", "c"]);
    let ticket = Uuid::from_u128(0x3300);
    let ticket_dir = f
        .root()
        .join(".workflow-tools/ticket/tickets")
        .join(ticket.to_string());
    fs::create_dir_all(&ticket_dir).unwrap();
    fs::write(
        ticket_dir.join("ticket.toml"),
        format!("id = \"{ticket}\"\ncreated_at = \"2026-10-29T00:00:00Z\"\ntitle = \"Existing Ticket\"\nstate = \"ready\"\n"),
    ).unwrap();
    assert_eq!(
        apply(
            &f,
            "g",
            2,
            PlanOperation::SetGoal {
                title: "Retitled".into(),
                objective: "New goal".into()
            }
        )
        .unwrap()
        .revision,
        3
    );
    let mut updated = waypoint("a");
    updated.prompt = "changed".into();
    updated.ownership_refs.ticket_id = Some(ticket);
    assert_eq!(
        apply(
            &f,
            "u",
            3,
            PlanOperation::UpdateWaypoint { waypoint: updated }
        )
        .unwrap()
        .revision,
        4
    );
    assert_eq!(
        apply(
            &f,
            "c",
            4,
            PlanOperation::CreateWaypoint {
                waypoint: waypoint("d"),
                after: Some("a".into())
            }
        )
        .unwrap()
        .revision,
        5
    );
    assert_eq!(source_ids(&f), ["a", "d", "b", "c"]);
    assert_eq!(
        apply(
            &f,
            "o",
            5,
            PlanOperation::ReorderWaypoints {
                order: vec!["c".into(), "a".into(), "d".into(), "b".into()]
            }
        )
        .unwrap()
        .revision,
        6
    );
    let source = f.lifecycle.get_mission_source(f.id).unwrap();
    assert_eq!(source.revision, 6);
    assert_eq!(
        (source.title.as_str(), source.objective.as_str()),
        ("Retitled", "New goal")
    );
    let chain: Vec<_> = source
        .waypoints
        .iter()
        .map(|w| (w.waypoint_id.as_str(), w.depends_on.clone()))
        .collect();
    assert_eq!(
        chain,
        [
            ("c", vec![]),
            ("a", vec!["c".to_string()]),
            ("d", vec!["a".to_string()]),
            ("b", vec!["d".to_string()])
        ]
    );
    let accepted = f.lifecycle.get(f.id).unwrap();
    assert_eq!(accepted.bundle.manifest.revision, 6);
    assert_eq!(
        accepted.bundle.manifest.execution_order,
        ["c", "a", "d", "b"]
    );
    let plan = review(&f);
    assert_eq!(plan["revision"], 6);
    assert_eq!(projection(&f), plan);
    let refs = plan["waypoints"][1]["references"].as_array().unwrap();
    assert!(
        refs.iter()
            .any(|r| r.as_str().unwrap().contains(&ticket.to_string()))
    );
}

#[test]
fn invalid_reorder_is_rejected_without_changes() {
    let f = seeded(3, &["a", "b", "c"]);
    for (n, order) in [
        ("missing", vec!["a", "b"]),
        ("duplicate", vec!["a", "a", "b", "c"]),
    ] {
        rejected(
            &f,
            n,
            2,
            PlanOperation::ReorderWaypoints {
                order: order.into_iter().map(str::to_owned).collect(),
            },
            "invalid_order",
        );
    }
}

#[test]
fn removal_of_referenced_work_needs_a_same_operation_replacement_and_keeps_results() {
    let f = seeded(3, &["a", "b", "c"]);
    rejected(
        &f,
        "rm",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: None,
        },
        "replacement_required",
    );
    rejected(
        &f,
        "dup",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: Some(waypoint("c")),
        },
        "invalid-part-reference",
    );
    let before = review(&f)["plan_digest"].clone();
    let o = apply(
        &f,
        "rm2",
        2,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "b".into(),
            replacement: Some(waypoint("b2")),
        },
    )
    .unwrap();
    assert_eq!(o.revision, 3);
    assert_eq!(source_ids(&f), ["a", "b2", "c"]);
    assert_eq!(
        f.lifecycle.get_mission_source(f.id).unwrap().waypoints[2].depends_on,
        ["b2"]
    );
    assert_ne!(Value::String(o.plan_digest.clone()), before);
    apply(
        &f,
        "rm3",
        3,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "c".into(),
            replacement: None,
        },
    )
    .unwrap();
    assert_eq!(source_ids(&f), ["a", "b2"]);
    let digest = review(&f)["plan_digest"].as_str().unwrap().to_owned();
    apply(
        &f,
        "ev",
        4,
        PlanOperation::RecordEvidence {
            plan_digest: digest,
            result: evidence("a"),
        },
    )
    .unwrap();
    rejected(
        &f,
        "rm4",
        5,
        PlanOperation::RemoveWaypoint {
            waypoint_id: "a".into(),
            replacement: Some(waypoint("a2")),
        },
        "retained-waypoint-required",
    );
    assert_eq!(source_ids(&f), ["a", "b2"]);
}

#[test]
fn v3_requires_complete_outcome_plans_and_legacy_reads_invent_nothing() {
    let f = seeded(3, &["a"]);
    let mut missing = waypoint("b");
    missing.outcome_plan = None;
    rejected(
        &f,
        "m",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: missing,
            after: None,
        },
        "invalid-outcome-plan",
    );
    let mut blank = waypoint("b");
    blank.outcome_plan.as_mut().unwrap().alternative_outcomes[0].acceptable_handling =
        vec![" ".into()];
    rejected(
        &f,
        "bl",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: blank,
            after: None,
        },
        "invalid-outcome-plan",
    );
    apply(
        &f,
        "ok",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: waypoint("b"),
            after: None,
        },
    )
    .unwrap();
    let accepted = f.lifecycle.get(f.id).unwrap();
    assert_eq!(accepted.bundle.manifest.schema_version, 2);
    assert_eq!(
        accepted.bundle.waypoints[1]
            .outcome_plan
            .as_ref()
            .unwrap()
            .expected_outcome,
        "b is ready"
    );
    let old = seeded(2, &["a"]);
    let plan = review(&old);
    assert!(plan["requirements"].as_array().unwrap().is_empty());
    assert!(
        plan["waypoints"][0]
            .get("outcome_plan")
            .is_none_or(Value::is_null)
    );
    rejected(
        &old,
        "up",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: waypoint("b"),
            after: None,
        },
        "invalid-outcome-plan",
    );
    apply(
        &old,
        "lg",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: legacy("b"),
            after: None,
        },
    )
    .unwrap();
    assert_eq!(
        old.lifecycle
            .get_mission_source(old.id)
            .unwrap()
            .schema_version,
        2
    );
    assert!(review(&old)["requirements"].as_array().unwrap().is_empty());
}

#[test]
fn requirement_updates_validate_owners_and_ownership_coverage() {
    let f = seeded(3, &["a", "b"]);
    let requirement = |id: &str, deferred| Requirement {
        id: id.into(),
        text: format!("{id} text"),
        deferred,
        canonical_criteria: vec![],
    };
    rejected(
        &f,
        "q1",
        2,
        PlanOperation::SetRequirements {
            requirements: vec![PlanRequirement {
                requirement: requirement("req-one", false),
                owner_waypoint_ids: vec!["ghost".into()],
            }],
        },
        "waypoint_not_found",
    );
    rejected(
        &f,
        "q2",
        2,
        PlanOperation::SetRequirements {
            requirements: vec![PlanRequirement {
                requirement: requirement("req-one", false),
                owner_waypoint_ids: vec![],
            }],
        },
        "unowned-requirement",
    );
    let before = review(&f)["plan_digest"].clone();
    let o = apply(
        &f,
        "q3",
        2,
        PlanOperation::SetRequirements {
            requirements: vec![
                PlanRequirement {
                    requirement: requirement("req-one", false),
                    owner_waypoint_ids: vec!["a".into()],
                },
                PlanRequirement {
                    requirement: requirement("req-two", true),
                    owner_waypoint_ids: vec![],
                },
            ],
        },
    )
    .unwrap();
    assert_eq!(o.revision, 3);
    assert_ne!(Value::String(o.plan_digest), before);
    assert_eq!(review(&f)["requirements"].as_array().unwrap().len(), 2);
    assert_eq!(
        f.lifecycle.get(f.id).unwrap().bundle.waypoints[0].requirement_ids,
        ["req-one"]
    );
}

#[test]
fn review_plan_is_deterministic_complete_and_evidence_is_bound_to_plan_identity() {
    let f = seeded(3, &["a", "b"]);
    let plan = review(&f);
    assert_eq!(plan, review(&f));
    assert_eq!(
        (plan["title"].as_str(), plan["objective"].as_str()),
        (Some("Plan"), Some("Goal"))
    );
    assert_eq!(ids(&plan), ["a", "b"]);
    let first = &plan["waypoints"][0];
    assert_eq!(first["non_goals"][0], "a non-goal");
    assert_eq!(first["validation"][0]["command"], "cargo test");
    assert_eq!(first["outcome_plan"]["expected_outcome"], "a is ready");
    assert_eq!(first["review_status"], "pending");
    assert_eq!(plan["waypoints"][1]["depends_on"][0], "a");
    let digest = plan["plan_digest"].as_str().unwrap().to_owned();
    assert_eq!(digest.len(), 64);
    assert!(digest.bytes().all(|b| b.is_ascii_hexdigit()));
    assert_eq!(projection(&f), plan);
    assert_eq!(
        review(&seeded(3, &["a", "b"]))["plan_digest"],
        plan["plan_digest"]
    );
    rejected(
        &f,
        "stale",
        2,
        PlanOperation::RecordEvidence {
            plan_digest: "0".repeat(64),
            result: evidence("a"),
        },
        "revision_conflict",
    );
    let source_before = f.lifecycle.get_mission_source(f.id).unwrap();
    let o = apply(
        &f,
        "ev",
        2,
        PlanOperation::RecordEvidence {
            plan_digest: digest.clone(),
            result: evidence("a"),
        },
    )
    .unwrap();
    assert_eq!((o.revision, o.plan_digest.as_str()), (3, digest.as_str()));
    let after = review(&f);
    assert_eq!(after["plan_digest"], plan["plan_digest"]);
    assert_ne!(after["evidence_identity"], plan["evidence_identity"]);
    assert_eq!(after["waypoints"][0]["review_status"], "evidenced");
    assert_eq!(projection(&f), after);
    let source_after = f.lifecycle.get_mission_source(f.id).unwrap();
    assert_eq!(source_after.waypoints, source_before.waypoints);
    assert_eq!(source_after.revision, 3);
    let mut edited = waypoint("b");
    edited.prompt = "semantic edit".into();
    let o = apply(
        &f,
        "edit",
        3,
        PlanOperation::UpdateWaypoint { waypoint: edited },
    )
    .unwrap();
    assert_ne!(o.plan_digest, digest);
    assert_eq!(review(&f)["plan_digest"], Value::String(o.plan_digest));
}

#[test]
fn blocked_publication_preserves_prior_set_and_succeeds_coherently_after_recovery() {
    let f = seeded(3, &["a"]);
    let backup = f
        .mission_dir()
        .parent()
        .unwrap()
        .join(format!(".{}.typed.backup", f.id));
    fs::create_dir(&backup).unwrap();
    rejected(
        &f,
        "blocked",
        2,
        PlanOperation::CreateWaypoint {
            waypoint: waypoint("b"),
            after: None,
        },
        "publication_recovery",
    );
    fs::remove_dir(&backup).unwrap();
    assert_eq!(
        apply(
            &f,
            "retry",
            2,
            PlanOperation::CreateWaypoint {
                waypoint: waypoint("b"),
                after: None
            }
        )
        .unwrap()
        .revision,
        3
    );
    assert_eq!(f.lifecycle.get(f.id).unwrap().bundle.manifest.revision, 3);
    assert_eq!(f.lifecycle.get_mission_source(f.id).unwrap().revision, 3);
    assert_eq!(projection(&f)["revision"], 3);
}
