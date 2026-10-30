use super::*;

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
