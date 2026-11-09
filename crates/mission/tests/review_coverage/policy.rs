use super::*;

#[test]
fn working_behavior_rejects_documentation_only_and_mock_requires_allowance() {
    let documentation = route(SOURCE, WaypointWorkKind::Documentation);
    let intent = confirm_goal(
        &documentation,
        IntentOutcomeCategory::WorkingBehavior,
        false,
    );
    let cold = input(
        &documentation,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold",
        None,
    );
    assert_eq!(
        documentation
            .approval()
            .record_review(documentation.id, cold)
            .unwrap_err()
            .code(),
        "approval_goal_coverage_invalid"
    );
    let formal = input(
        &documentation,
        &intent,
        ReviewKind::Formal,
        ReviewVerdict::Pass,
        "formal",
        Some("review-00000001".into()),
    );
    assert_eq!(
        documentation
            .approval()
            .record_review(documentation.id, formal)
            .unwrap_err()
            .code(),
        "approval_goal_coverage_invalid"
    );

    let mocked = route(SOURCE, WaypointWorkKind::Implementation);
    let disallow = confirm_goal(&mocked, IntentOutcomeCategory::WorkingBehavior, false);
    let mut rejected = input(
        &mocked,
        &disallow,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "mock-disallowed",
        None,
    );
    let GoalCoverageDispositionV4::Fulfilled {
        acceptance_checks, ..
    } = &mut rejected.goal_coverage_findings[0].disposition
    else {
        unreachable!()
    };
    acceptance_checks[0].evidence_class = PlannedEvidenceClassV4::MockedBehavior;
    assert_eq!(
        mocked
            .approval()
            .record_review(mocked.id, rejected)
            .unwrap_err()
            .code(),
        "approval_goal_coverage_invalid"
    );
    let allowed = route(SOURCE, WaypointWorkKind::Implementation);
    let allow = confirm_goal(&allowed, IntentOutcomeCategory::WorkingBehavior, true);
    let mut accepted = input(
        &allowed,
        &allow,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "mock-allowed",
        None,
    );
    let GoalCoverageDispositionV4::Fulfilled {
        acceptance_checks, ..
    } = &mut accepted.goal_coverage_findings[0].disposition
    else {
        unreachable!()
    };
    acceptance_checks[0].evidence_class = PlannedEvidenceClassV4::MockedBehavior;
    assert!(
        allowed
            .approval()
            .record_review(allowed.id, accepted)
            .is_ok()
    );
}

#[test]
fn explicit_artifact_design_and_receipted_exclusion_are_compatible() {
    let artifact = route(
        b"Create a search design document",
        WaypointWorkKind::Documentation,
    );
    let artifact_intent = confirm_goal(&artifact, IntentOutcomeCategory::Artifact, false);
    let cold = record(
        &artifact,
        input(
            &artifact,
            &artifact_intent,
            ReviewKind::ColdWalkthrough,
            ReviewVerdict::Pass,
            "cold",
            None,
        ),
        "artifact-cold",
    );
    let formal = record(
        &artifact,
        input(
            &artifact,
            &artifact_intent,
            ReviewKind::Formal,
            ReviewVerdict::Pass,
            "formal",
            Some(cold.review_id.clone()),
        ),
        "artifact-formal",
    );
    assert!(cold.source_goal_binding_sha256.is_some());
    assert_eq!(
        formal
            .terminal_outcome_finding
            .as_ref()
            .unwrap()
            .cold_review_id
            .as_deref(),
        Some(cold.review_id.as_str())
    );
    let wrong_cold = input(
        &artifact,
        &artifact_intent,
        ReviewKind::Formal,
        ReviewVerdict::Pass,
        "other-formal",
        Some("review-99999999".into()),
    );
    assert_eq!(
        artifact
            .approval()
            .record_review(artifact.id, wrong_cold)
            .unwrap_err()
            .code(),
        "approval_required"
    );

    let excluded = route(SOURCE, WaypointWorkKind::Implementation);
    let original = confirm_goal(&excluded, IntentOutcomeCategory::WorkingBehavior, false);
    let mut unapproved = input(
        &excluded,
        &original,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "unapproved-exclusion",
        None,
    );
    unapproved.goal_coverage_findings[0].disposition = GoalCoverageDispositionV4::Excluded {
        amendment_id: "agent-non-goal".into(),
        prior_requirement_id: "search".into(),
    };
    assert_eq!(
        excluded
            .approval()
            .record_review(excluded.id, unapproved)
            .unwrap_err()
            .code(),
        "approval_goal_coverage_invalid"
    );

    let amendment_id = "requester-authorized-exclusion";
    let reference = IntentReceiptReference {
        session_id: approval_fixture::SESSION.into(),
        challenge_id: Uuid::new_v4(),
    };
    let source = "Requester authorizes excluding the search requirement.";
    let changes = vec![IntentChangeV4::Exclude {
        prior_requirement_id: "search".into(),
    }];
    let revision = excluded.plan().revision;
    let (_, prepared) = execute_intent(
        &excluded.ws,
        excluded.id,
        IntentOperation::PrepareAmendment {
            expected_revision: revision,
            amendment_id: amendment_id.into(),
            prior_chain_sha256: original.chain_sha256.clone(),
            source: source.into(),
            changes: changes.clone(),
            reference: reference.clone(),
        },
    )
    .unwrap();
    let IntentOutput::Challenge { prompt, .. } = prepared else {
        panic!("amendment preparation must return a requester challenge");
    };
    capture(&excluded, prompt);
    execute_intent(
        &excluded.ws,
        excluded.id,
        IntentOperation::AppendAmendment {
            expected_revision: revision,
            amendment_id: amendment_id.into(),
            prior_chain_sha256: original.chain_sha256.clone(),
            source: source.into(),
            changes,
            reference,
        },
    )
    .unwrap();
    let updated = MissionStore::open(&excluded.ws)
        .effective_intent(
            excluded.id,
            &SessionIntentProvider::new(&excluded.ws).unwrap(),
        )
        .unwrap()
        .unwrap();
    let mut exclusion = input(
        &excluded,
        &original,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "authorized-exclusion",
        None,
    );
    exclusion.terminal_outcome_finding = Some(TerminalOutcomeFindingV4 {
        source_sha256: updated.baseline.source_sha256.clone(),
        baseline_sha256: updated.baseline.baseline_sha256.clone(),
        amendment_chain_sha256: updated.chain_sha256.clone(),
        plan_revision: excluded.plan().revision,
        plan_digest: excluded.plan().plan_digest,
        cold_review_id: None,
        conclusion: TerminalOutcomeConclusionV4::ReachesRequesterTarget,
        observable_terminal_outcome: "No active search requirements remain.".into(),
    });
    exclusion.goal_coverage_findings[0] = GoalCoverageFindingV4 {
        requirement_id: "search".into(),
        disposition: GoalCoverageDispositionV4::Excluded {
            amendment_id: amendment_id.into(),
            prior_requirement_id: "search".into(),
        },
    };
    record(&excluded, exclusion, "authorized-exclusion");
}

#[test]
fn decision_and_research_outcomes_require_their_matching_evidence_routes() {
    let decision = route(SOURCE, WaypointWorkKind::Documentation);
    let decision_intent = confirm_goal(&decision, IntentOutcomeCategory::Decision, false);
    let mut decision_review = input(
        &decision,
        &decision_intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "decision",
        None,
    );
    let GoalCoverageDispositionV4::Fulfilled {
        acceptance_checks, ..
    } = &mut decision_review.goal_coverage_findings[0].disposition
    else {
        unreachable!()
    };
    acceptance_checks[0].evidence_class = PlannedEvidenceClassV4::CapturedDecision;
    assert!(
        decision
            .approval()
            .record_review(decision.id, decision_review)
            .is_ok()
    );

    let research = route(SOURCE, WaypointWorkKind::Research);
    let research_intent = confirm_goal(&research, IntentOutcomeCategory::ResearchReview, false);
    let research_review = input(
        &research,
        &research_intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "research",
        None,
    );
    assert!(
        research
            .approval()
            .record_review(research.id, research_review)
            .is_ok()
    );
}
