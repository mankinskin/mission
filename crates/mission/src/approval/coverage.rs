use super::*;
use mission_api::{
    BaselineRequirementV4, IntentChangeV4, IntentEvidencePolicy, IntentOutcomeCategory,
    IntentSourceSpan, WaypointWorkKind,
};
use std::collections::{BTreeMap, BTreeSet};

type SourceRequirements = (
    BTreeMap<String, BaselineRequirementV4>,
    BTreeMap<String, String>,
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct GoalCoverageFindingV4 {
    pub requirement_id: String,
    pub disposition: GoalCoverageDispositionV4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(tag = "disposition", rename_all = "snake_case", deny_unknown_fields)]
pub enum GoalCoverageDispositionV4 {
    Fulfilled {
        source: IntentSourceSpan,
        observable_terminal_outcome: String,
        owner_waypoint_ids: Vec<String>,
        acceptance_checks: Vec<PlannedAcceptanceCheckV4>,
    },
    Excluded {
        amendment_id: String,
        prior_requirement_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PlannedEvidenceClassV4 {
    ExecutableBehavior,
    MockedBehavior,
    DeterministicArtifact,
    CapturedDecision,
    ResearchRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PlannedAcceptanceCheckV4 {
    pub waypoint_id: String,
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    pub evidence_class: PlannedEvidenceClassV4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcomeConclusionV4 {
    ReachesRequesterTarget,
    DoesNotReachRequesterTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct TerminalOutcomeFindingV4 {
    pub source_sha256: String,
    pub baseline_sha256: String,
    pub amendment_chain_sha256: String,
    pub plan_revision: u64,
    pub plan_digest: String,
    #[serde(default)]
    pub cold_review_id: Option<String>,
    pub conclusion: TerminalOutcomeConclusionV4,
    pub observable_terminal_outcome: String,
}

pub(super) fn validate_source_goal(
    intent: Option<&EffectiveIntentV4>,
    plan: &ReviewPlan,
    input: &ReviewInput,
) -> Result<(), ApprovalError> {
    let Some(intent) = intent else {
        if input.terminal_outcome_finding.is_some() || !input.goal_coverage_findings.is_empty() {
            return Err(stale(
                "source-goal findings cannot be applied without confirmed intent authority",
            ));
        }
        return Ok(());
    };
    let terminal = input
        .terminal_outcome_finding
        .as_ref()
        .ok_or_else(|| required("a confirmed intent requires a terminal-outcome finding"))?;
    if terminal.source_sha256 != intent.baseline.source_sha256
        || terminal.baseline_sha256 != intent.baseline.baseline_sha256
        || terminal.amendment_chain_sha256 != intent.chain_sha256
        || terminal.plan_revision != plan.revision
        || terminal.plan_digest != plan.plan_digest
    {
        return Err(stale(
            "terminal-outcome finding is bound to a different source, intent chain or plan",
        ));
    }
    if terminal.observable_terminal_outcome.trim().is_empty() {
        return Err(required(
            "terminal-outcome finding must describe the observable final outcome",
        ));
    }
    match (input.kind, terminal.cold_review_id.as_deref()) {
        (ReviewKind::ColdWalkthrough, None) => {}
        (ReviewKind::Formal, Some(cold_id)) if input.cold_review_id.as_deref() == Some(cold_id) => {
        }
        (ReviewKind::ColdWalkthrough, Some(_)) => {
            return Err(required(
                "a cold walkthrough cannot reference another review",
            ));
        }
        (ReviewKind::Formal, _) => {
            return Err(required(
                "a formal terminal finding must reference its cold walkthrough",
            ));
        }
    }
    if input.verdict == ReviewVerdict::Pass
        && terminal.conclusion != TerminalOutcomeConclusionV4::ReachesRequesterTarget
    {
        return Err(ApprovalError::new(
            "approval_source_goal_not_met",
            "a passing review must conclude that successful waypoints reach the requester target",
        ));
    }
    validate_coverage_findings(intent, plan, &input.goal_coverage_findings)
}

fn source_requirements(intent: &EffectiveIntentV4) -> Result<SourceRequirements, ApprovalError> {
    let mut requirements = BTreeMap::new();
    for requirement in &intent.baseline.requirements {
        if requirements
            .insert(requirement.requirement_id.clone(), requirement.clone())
            .is_some()
        {
            return Err(invalid("source requirement IDs are not unique"));
        }
    }
    let mut exclusions = BTreeMap::new();
    for amendment in &intent.amendments {
        for change in &amendment.changes {
            match change {
                IntentChangeV4::Exclude {
                    prior_requirement_id,
                } => {
                    if exclusions
                        .insert(prior_requirement_id.clone(), amendment.amendment_id.clone())
                        .is_some()
                    {
                        return Err(invalid("source requirement has duplicate exclusions"));
                    }
                }
                IntentChangeV4::Replace {
                    prior_requirement_id,
                    requirement,
                } => {
                    if exclusions
                        .insert(prior_requirement_id.clone(), amendment.amendment_id.clone())
                        .is_some()
                        || requirements
                            .insert(requirement.requirement_id.clone(), requirement.clone())
                            .is_some()
                    {
                        return Err(invalid("source amendment IDs are not unique"));
                    }
                }
                IntentChangeV4::Add { requirement } => {
                    if requirements
                        .insert(requirement.requirement_id.clone(), requirement.clone())
                        .is_some()
                    {
                        return Err(invalid("source amendment IDs are not unique"));
                    }
                }
            }
        }
    }
    if exclusions
        .keys()
        .any(|id| !intent.excluded_requirement_ids.contains(id))
        || intent
            .excluded_requirement_ids
            .iter()
            .any(|id| !exclusions.contains_key(id))
    {
        return Err(invalid(
            "effective exclusions do not match requester-receipted amendments",
        ));
    }
    let active_ids: BTreeSet<_> = intent
        .requirements
        .iter()
        .map(|requirement| requirement.requirement_id.as_str())
        .collect();
    if requirements
        .keys()
        .filter(|id| !exclusions.contains_key(*id))
        .any(|id| !active_ids.contains(id.as_str()))
        || active_ids.iter().any(|id| !requirements.contains_key(*id))
    {
        return Err(invalid("effective source requirement set is inconsistent"));
    }
    Ok((requirements, exclusions))
}

fn validate_coverage_findings(
    intent: &EffectiveIntentV4,
    plan: &ReviewPlan,
    findings: &[GoalCoverageFindingV4],
) -> Result<(), ApprovalError> {
    let (requirements, exclusions) = source_requirements(intent)?;
    let mut seen = BTreeSet::new();
    if findings.len() != requirements.len() {
        return Err(required(
            "each source requirement needs exactly one coverage disposition",
        ));
    }
    for finding in findings {
        if !seen.insert(finding.requirement_id.as_str()) {
            return Err(invalid("source requirement has duplicate dispositions"));
        }
        let requirement = requirements
            .get(&finding.requirement_id)
            .ok_or_else(|| invalid("coverage finding names an unknown source requirement"))?;
        match &finding.disposition {
            GoalCoverageDispositionV4::Fulfilled {
                source,
                observable_terminal_outcome,
                owner_waypoint_ids,
                acceptance_checks,
            } => {
                if exclusions.contains_key(&finding.requirement_id) {
                    return Err(invalid(
                        "an excluded requirement cannot be reported as fulfilled",
                    ));
                }
                validate_fulfilled(
                    requirement,
                    plan,
                    source,
                    observable_terminal_outcome,
                    owner_waypoint_ids,
                    acceptance_checks,
                )?;
            }
            GoalCoverageDispositionV4::Excluded {
                amendment_id,
                prior_requirement_id,
            } => {
                if prior_requirement_id != &finding.requirement_id
                    || exclusions.get(prior_requirement_id) != Some(amendment_id)
                {
                    return Err(invalid(
                        "exclusion must cite the requester-receipted amendment for this prior requirement",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn validate_fulfilled(
    requirement: &BaselineRequirementV4,
    plan: &ReviewPlan,
    source: &IntentSourceSpan,
    outcome: &str,
    owners: &[String],
    checks: &[PlannedAcceptanceCheckV4],
) -> Result<(), ApprovalError> {
    if source != &requirement.source || outcome.trim().is_empty() {
        return Err(invalid(
            "fulfilled finding must preserve the immutable source reference and observable outcome",
        ));
    }
    if owners.is_empty()
        || owners.iter().collect::<BTreeSet<_>>().len() != owners.len()
        || owners
            .iter()
            .any(|id| !plan.waypoints.iter().any(|w| &w.waypoint_id == id))
    {
        return Err(invalid(
            "fulfilled finding must name unique existing owner waypoints",
        ));
    }
    if checks.is_empty() {
        return Err(required(
            "fulfilled finding must include concrete planned acceptance checks",
        ));
    }
    let mut seen_checks = BTreeSet::new();
    let mut has_implementation_check = false;
    for check in checks {
        let key = (
            check.waypoint_id.as_str(),
            check.command.as_str(),
            check.cwd.as_deref(),
        );
        if !seen_checks.insert(key)
            || check.command.trim().is_empty()
            || check
                .cwd
                .as_deref()
                .is_some_and(|cwd| cwd.trim().is_empty())
            || !owners.iter().any(|id| id == &check.waypoint_id)
        {
            return Err(invalid(
                "acceptance checks must be unique and point to a named owner waypoint",
            ));
        }
        let waypoint = plan
            .waypoints
            .iter()
            .find(|waypoint| waypoint.waypoint_id == check.waypoint_id)
            .ok_or_else(|| invalid("acceptance check names an unknown waypoint"))?;
        if !waypoint
            .validation
            .iter()
            .any(|gate| gate.command == check.command && gate.cwd == check.cwd)
        {
            return Err(invalid(
                "acceptance command is not declared by its owner waypoint",
            ));
        }
        let compatible = match (
            &requirement.category,
            &requirement.evidence_policy,
            &check.evidence_class,
        ) {
            (
                IntentOutcomeCategory::WorkingBehavior,
                IntentEvidencePolicy::ExecutableBehavior { .. },
                PlannedEvidenceClassV4::ExecutableBehavior,
            ) => {
                if waypoint.work_kind != Some(WaypointWorkKind::Implementation) {
                    return Err(invalid(
                        "executable behavior checks require an implementation waypoint",
                    ));
                }
                has_implementation_check = true;
                true
            }
            (
                IntentOutcomeCategory::WorkingBehavior,
                IntentEvidencePolicy::ExecutableBehavior { allow_mock: true },
                PlannedEvidenceClassV4::MockedBehavior,
            ) => {
                if waypoint.work_kind != Some(WaypointWorkKind::Implementation) {
                    return Err(invalid(
                        "mocked behavior checks require an implementation waypoint",
                    ));
                }
                has_implementation_check = true;
                true
            }
            (
                IntentOutcomeCategory::Artifact,
                IntentEvidencePolicy::DeterministicArtifact,
                PlannedEvidenceClassV4::DeterministicArtifact,
            )
            | (
                IntentOutcomeCategory::Decision,
                IntentEvidencePolicy::CapturedDecision,
                PlannedEvidenceClassV4::CapturedDecision,
            ) => true,
            (
                IntentOutcomeCategory::ResearchReview,
                IntentEvidencePolicy::ResearchRecord,
                PlannedEvidenceClassV4::ResearchRecord,
            ) => waypoint.work_kind == Some(WaypointWorkKind::Research),
            _ => false,
        };
        if !compatible {
            return Err(invalid(
                "acceptance evidence class is incompatible with the confirmed outcome policy",
            ));
        }
    }
    if requirement.category == IntentOutcomeCategory::WorkingBehavior && !has_implementation_check {
        return Err(invalid(
            "WorkingBehavior requires an implementation waypoint and executable behavior check",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> ApprovalError {
    ApprovalError::new("approval_goal_coverage_invalid", message)
}

pub(super) fn same_source_snapshot(
    cold: &ReviewRecord,
    terminal: &TerminalOutcomeFindingV4,
) -> bool {
    let Some(cold_terminal) = cold.terminal_outcome_finding.as_ref() else {
        return false;
    };
    cold.source_goal_binding_sha256.is_some()
        && cold_terminal.source_sha256 == terminal.source_sha256
        && cold_terminal.baseline_sha256 == terminal.baseline_sha256
        && cold_terminal.amendment_chain_sha256 == terminal.amendment_chain_sha256
        && cold_terminal.plan_revision == terminal.plan_revision
        && cold_terminal.plan_digest == terminal.plan_digest
}

pub(super) fn review_binding_sha256(
    record: &ReviewRecord,
) -> Result<Option<String>, ApprovalError> {
    let Some(terminal) = record.terminal_outcome_finding.as_ref() else {
        if !record.goal_coverage_findings.is_empty() || record.source_goal_binding_sha256.is_some()
        {
            return Err(corrupt("source-goal review binding is incomplete".into()));
        }
        return Ok(None);
    };
    let bytes = serde_json::to_vec(&(
        "mission-source-goal-review/v4",
        record.review_id.as_str(),
        record.kind,
        record.verdict,
        record.revision,
        record.plan_digest.as_str(),
        record.evidence_identity.as_str(),
        record.cold_review_id.as_deref(),
        terminal,
        &record.goal_coverage_findings,
    ))
    .map_err(|error| corrupt(error.to_string()))?;
    let serialized = String::from_utf8(bytes).map_err(|error| corrupt(error.to_string()))?;
    Ok(Some(sha(&serialized)))
}
