use super::*;

/// Native version requiring requester-authored planned outcomes.
pub const OUTCOME_BUNDLE_SCHEMA_VERSION: u32 = 2;
/// Typed version requiring requester-authored planned outcomes.
pub const OUTCOME_SOURCE_SCHEMA_VERSION: u32 = 3;

/// Descriptive plan-time guidance, never an actual result or execution policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaypointOutcomePlan {
    pub expected_outcome: String,
    pub alternative_outcomes: Vec<AlternativeOutcome>,
}

/// One possible alternative and its descriptive human handling options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlternativeOutcome {
    pub outcome: String,
    pub acceptable_handling: Vec<String>,
}

pub(super) fn validate_outcome_plan(
    plan: Option<&WaypointOutcomePlan>,
    required: bool,
    waypoint_id: &str,
) -> Result<(), Diagnostic> {
    let message = match plan {
        None if required => {
            Some("This schema requires a complete requester-authored outcome_plan.")
        }
        Some(_) if !required => Some(
            "Legacy schemas do not carry outcome_plan; explicitly upgrade with complete requester-authored plans.",
        ),
        Some(plan)
            if plan.expected_outcome.trim().is_empty()
                || plan.alternative_outcomes.is_empty()
                || plan.alternative_outcomes.iter().any(|alternative| {
                    alternative.outcome.trim().is_empty()
                        || alternative.acceptable_handling.is_empty()
                        || alternative
                            .acceptable_handling
                            .iter()
                            .any(|text| text.trim().is_empty())
                }) =>
        {
            Some(
                "outcome_plan requires nonblank expected_outcome, at least one nonblank alternative outcome, and nonempty nonblank acceptable_handling options.",
            )
        }
        _ => None,
    };
    match message {
        Some(message) => Err(Diagnostic {
            code: "invalid-outcome-plan",
            source: "waypoint".into(),
            pointer: "/outcome_plan".into(),
            waypoint_id: Some(waypoint_id.into()),
            message: message.into(),
        }),
        None => Ok(()),
    }
}

pub(super) fn render_outcome_plan(output: &mut String, plan: Option<&WaypointOutcomePlan>) {
    let Some(plan) = plan else { return };
    output.push_str("## Planned outcomes (manual guidance)\n\nDescriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.\n\n");
    output.push_str(&format!(
        "**Expected outcome:** {}\n\n",
        schema_renderers::markdown_text(&plan.expected_outcome)
    ));
    for (index, alternative) in plan.alternative_outcomes.iter().enumerate() {
        output.push_str(&format!(
            "**Alternative outcome {}:** {}\n\n**Acceptable handling (manual options):**\n\n",
            index + 1,
            schema_renderers::markdown_text(&alternative.outcome)
        ));
        for option in &alternative.acceptable_handling {
            output.push_str(&format!("- {}\n", schema_renderers::markdown_text(option)));
        }
        output.push('\n');
    }
}

pub(super) fn renderer_version(bundle: &MissionBundle) -> &'static str {
    if bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION {
        "mission-api/6"
    } else {
        RENDERER_VERSION
    }
}
