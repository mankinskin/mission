use super::*;

#[test]
fn confirmed_working_goal_cannot_pass_review_without_source_coverage() {
    let fx = route(SOURCE, WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);
    let mut review = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "cold",
        None,
    );
    review.goal_coverage_findings.clear();
    review.terminal_outcome_finding = None;
    let error = fx
        .approval()
        .record_review(fx.id, review)
        .expect_err("confirmed requester goals cannot be omitted from review");
    assert_eq!(error.code(), "approval_required");
}

#[test]
fn omitted_duplicate_dangling_and_stale_findings_reject_pass() {
    let fx = route(SOURCE, WaypointWorkKind::Implementation);
    let intent = confirm_goal(&fx, IntentOutcomeCategory::WorkingBehavior, false);

    let mut omitted = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "omitted",
        None,
    );
    omitted.goal_coverage_findings.clear();
    assert_eq!(
        fx.approval()
            .record_review(fx.id, omitted)
            .unwrap_err()
            .code(),
        "approval_required"
    );

    let mut missing_terminal = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "missing-terminal",
        None,
    );
    missing_terminal.terminal_outcome_finding = None;
    assert_eq!(
        fx.approval()
            .record_review(fx.id, missing_terminal)
            .unwrap_err()
            .code(),
        "approval_required"
    );

    let mut duplicate = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "duplicate",
        None,
    );
    duplicate
        .goal_coverage_findings
        .push(duplicate.goal_coverage_findings[0].clone());
    assert_eq!(
        fx.approval()
            .record_review(fx.id, duplicate)
            .unwrap_err()
            .code(),
        "approval_required"
    );

    let mut unknown = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "unknown",
        None,
    );
    let GoalCoverageDispositionV4::Fulfilled {
        owner_waypoint_ids,
        acceptance_checks,
        ..
    } = &mut unknown.goal_coverage_findings[0].disposition
    else {
        unreachable!()
    };
    owner_waypoint_ids[0] = "missing-waypoint".into();
    acceptance_checks[0].waypoint_id = "missing-waypoint".into();
    assert_eq!(
        fx.approval()
            .record_review(fx.id, unknown)
            .unwrap_err()
            .code(),
        "approval_goal_coverage_invalid"
    );

    let mut dangling = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "dangling",
        None,
    );
    let GoalCoverageDispositionV4::Fulfilled {
        acceptance_checks, ..
    } = &mut dangling.goal_coverage_findings[0].disposition
    else {
        unreachable!()
    };
    acceptance_checks[0].command = "cargo test unrelated::suite".into();
    assert_eq!(
        fx.approval()
            .record_review(fx.id, dangling)
            .unwrap_err()
            .code(),
        "approval_goal_coverage_invalid"
    );

    let mut stale_source = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "stale-source",
        None,
    );
    stale_source
        .terminal_outcome_finding
        .as_mut()
        .unwrap()
        .source_sha256 = "0".repeat(64);
    assert_eq!(
        fx.approval()
            .record_review(fx.id, stale_source)
            .unwrap_err()
            .code(),
        "approval_stale"
    );
    let mut stale_chain = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "stale-chain",
        None,
    );
    stale_chain
        .terminal_outcome_finding
        .as_mut()
        .unwrap()
        .amendment_chain_sha256 = "1".repeat(64);
    assert_eq!(
        fx.approval()
            .record_review(fx.id, stale_chain)
            .unwrap_err()
            .code(),
        "approval_stale"
    );
    let mut stale_plan = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "stale-plan",
        None,
    );
    stale_plan
        .terminal_outcome_finding
        .as_mut()
        .unwrap()
        .plan_digest = "2".repeat(64);
    assert_eq!(
        fx.approval()
            .record_review(fx.id, stale_plan)
            .unwrap_err()
            .code(),
        "approval_stale"
    );
    let mut stale_revision = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "stale-revision",
        None,
    );
    stale_revision
        .terminal_outcome_finding
        .as_mut()
        .unwrap()
        .plan_revision += 1;
    assert_eq!(
        fx.approval()
            .record_review(fx.id, stale_revision)
            .unwrap_err()
            .code(),
        "approval_stale"
    );

    let mut failed_goal = input(
        &fx,
        &intent,
        ReviewKind::ColdWalkthrough,
        ReviewVerdict::Pass,
        "failed-goal",
        None,
    );
    failed_goal
        .terminal_outcome_finding
        .as_mut()
        .unwrap()
        .conclusion = TerminalOutcomeConclusionV4::DoesNotReachRequesterTarget;
    assert_eq!(
        fx.approval()
            .record_review(fx.id, failed_goal)
            .unwrap_err()
            .code(),
        "approval_source_goal_not_met"
    );
}
