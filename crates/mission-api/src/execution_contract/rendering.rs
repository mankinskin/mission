use super::*;

pub(crate) fn render_execution_contract(
    output: &mut String,
    contract: Option<&WaypointExecutionContractV4>,
) {
    let Some(contract) = contract else { return };
    let text = schema_renderers::markdown_text;
    output.push_str("## Execution contract (declared; not executed by Mission)\n\n");
    output.push_str(&format!(
        "**Outcome:** {} ({:?})  \n**Work kind:** {:?}  \n**Rationale:** {}\n\n",
        text(&contract.expected_outcome),
        contract.outcome_category,
        contract.work_kind,
        text(&contract.rationale)
    ));
    output.push_str("**Authoritative context:**\n\n");
    for context in &contract.authoritative_context {
        output.push_str(&format!(
            "- `{}` — {} ({})\n",
            text(&context.id),
            text(&context.reference),
            text(&context.description)
        ));
    }
    output.push_str(&format!(
        "\n**Boundary:** {} — {}\n\n",
        text(&contract.boundary.task),
        contract
            .boundary
            .paths
            .iter()
            .map(|path| format!("`{}`", text(path)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    append_conditions(output, "Preconditions", &contract.preconditions);
    append_conditions(output, "Postconditions", &contract.postconditions);
    output.push_str("**Checks:**\n\n");
    for check in &contract.checks {
        output.push_str(&format!(
            "- `{}` ({:?}, {:?}): `{}` (cwd: `{}`)\n",
            text(&check.id),
            check.kind,
            check.evidence_class,
            text(&check.command),
            text(&check.cwd)
        ));
    }
    output.push_str(&format!(
        "\n**Regression checks:** {}\n\n**Evidence:** {}\n\n",
        contract.regression_check_ids.join(", "),
        contract
            .evidence_requirements
            .iter()
            .map(|evidence| format!(
                "{} ({:?}: {})",
                evidence.id,
                evidence.class,
                evidence.check_ids.join(", ")
            ))
            .collect::<Vec<_>>()
            .join("; ")
    ));
    output.push_str(&format!(
        "**Tools:** {:?}; read roots: {}; write roots: {}; permissions: {:?}\n\n",
        contract.tool_policy.actions,
        contract.tool_policy.read_roots.join(", "),
        if contract.tool_policy.write_roots.is_empty() {
            contract
                .tool_policy
                .no_write_reason
                .as_deref()
                .unwrap_or("unspecified")
                .to_owned()
        } else {
            contract.tool_policy.write_roots.join(", ")
        },
        contract.tool_policy.permissions
    ));
    output.push_str(&format!(
        "**Environment:** {} — {} (preflight: {})\n\n**Failure handling:** when {}, {}; at most {} attempt(s), then {:?} to {}.\n\n",
        text(&contract.environment.profile),
        text(&contract.environment.description),
        contract.environment.preflight_check_ids.join(", "),
        text(&contract.failure_policy.trigger),
        text(&contract.failure_policy.diagnostic_action),
        contract.failure_policy.max_attempts,
        contract.failure_policy.exhaustion,
        text(&contract.failure_policy.escalation_target)
    ));
    if let Some(handover) = &contract.handover {
        output.push_str(&format!(
            "**Handover `{}`:** `{}` → `{}`; schema `{}`; producer `{}` at `{}`; consumer `{}` at `{}`.\n\n",
            text(&handover.id),
            text(&handover.producer.artifact_id),
            text(&handover.consumer.artifact_id),
            text(&handover.producer.schema),
            text(&handover.producer.content_reference),
            text(&handover.producer.root),
            text(&handover.consumer.content_reference),
            text(&handover.consumer.root)
        ));
    } else {
        output.push_str(&format!(
            "**No handover:** {}\n\n",
            text(contract.no_handover_reason.as_deref().unwrap_or(""))
        ));
    }
    if let Some(provider) = &contract.provider_route {
        output.push_str(&format!(
            "**Provider route:** {} v{} ({}) via {}\n\n",
            text(&provider.provider),
            text(&provider.version),
            text(&provider.receipt_class),
            provider.preflight_check_ids.join(", ")
        ));
    } else {
        output.push_str(&format!(
            "**No provider route:** {}\n\n",
            text(contract.no_provider_reason.as_deref().unwrap_or(""))
        ));
    }
}

fn append_conditions(output: &mut String, title: &str, conditions: &[ExecutionConditionV4]) {
    let text = schema_renderers::markdown_text;
    output.push_str(&format!("**{title}:**\n\n"));
    for condition in conditions {
        output.push_str(&format!(
            "- `{}` — {} (checks: {})\n",
            text(&condition.id),
            text(&condition.description),
            condition.check_ids.join(", ")
        ));
    }
    output.push('\n');
}
