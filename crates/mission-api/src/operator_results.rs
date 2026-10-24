use super::{Diagnostic, MissionBundle, OperatorResult, diag, escape};
use std::path::Path;

pub(super) fn validate(bundle: &MissionBundle) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for (index, result) in bundle.manifest.operator_results.iter().enumerate() {
        if !bundle
            .waypoints
            .iter()
            .any(|item| item.waypoint_id == result.waypoint_id)
        {
            diagnostics.push(diag(
                "unknown-result-waypoint",
                Path::new("manifest.json"),
                &format!("/operator_results/{index}/waypoint_id"),
                Some(&result.waypoint_id),
                "Operator results must reference a waypoint in the accepted Mission.",
            ));
        }
        if !matches!(result.outcome.as_str(), "passed" | "failed" | "blocked")
            || result.operator_command.trim().is_empty()
            || result.evidence_pointer.trim().is_empty()
        {
            diagnostics.push(diag(
                "invalid-operator-result",
                Path::new("manifest.json"),
                &format!("/operator_results/{index}"),
                Some(&result.waypoint_id),
                "Supply outcome passed, failed, or blocked, a nonblank operator_command, and a nonblank evidence_pointer. Recording evidence never executes a command.",
            ));
        }
    }
    diagnostics
}

pub(super) fn render(output: &mut String, results: &[OperatorResult], waypoint_id: Option<&str>) {
    let mut results = results
        .iter()
        .filter(|result| waypoint_id.is_none_or(|id| result.waypoint_id == id))
        .peekable();
    if results.peek().is_none() {
        return;
    }
    output.push_str("\n## Operator Results\n\n| Waypoint | Outcome | Operator command | Evidence |\n|---|---|---|---|\n");
    for result in results {
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            escape(&result.waypoint_id),
            escape(&result.outcome),
            escape(&result.operator_command),
            escape(&result.evidence_pointer),
        ));
    }
    output.push('\n');
}
