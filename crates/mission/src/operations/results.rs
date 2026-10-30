use super::*;

pub(crate) fn output_result(
    mission_id: Uuid,
    result: Result<OutputRecord, MissionError>,
) -> StatusSnapshot {
    match result {
        Ok(record) => {
            let revision = match &record {
                OutputRecord::Contract { contract, .. } => contract.revision,
                OutputRecord::ApprovalEvent(event) => event.reaction.revision,
            };
            let mut snapshot = success(Some(mission_id), Some(revision));
            snapshot.output_record = Some(record);
            snapshot
        }
        Err(error) => failure(Some(mission_id), error),
    }
}

pub(crate) fn repair_result(result: Result<RepairResponse, MissionError>) -> StatusSnapshot {
    match result {
        Ok(repair) => StatusSnapshot {
            status: if repair.status == "blocked" || repair.status == "partial" {
                "error".into()
            } else {
                "ok".into()
            },
            mission_id: None,
            revision: None,
            mission_state: None,
            publication_state: None,
            diagnostics: repair
                .diagnostics
                .iter()
                .map(|message| StatusDiagnostic {
                    code: "repair-operation".into(),
                    source: "repair".into(),
                    pointer: String::new(),
                    waypoint_id: None,
                    message: message.clone(),
                })
                .collect(),
            rendered: None,
            accepted: None,
            source: None,
            migration_report: None,
            typed_migration: None,
            repair: Some(repair),
            output_record: None,
            external_status_snapshot: BTreeMap::new(),
        },
        Err(error) => failure(None, error),
    }
}

/// CLI adapter boundary; behavior is intentionally identical to MCP.
pub fn execute_cli(workspace_root: &Path, operation: Operation) -> StatusSnapshot {
    execute(workspace_root, operation)
}

/// MCP adapter boundary; behavior is intentionally identical to CLI.
pub fn execute_mcp(workspace_root: &Path, operation: Operation) -> StatusSnapshot {
    execute(workspace_root, operation)
}

pub(crate) fn typed_result(result: Result<TypedDraft, MissionError>) -> StatusSnapshot {
    match result {
        Ok(source) => StatusSnapshot {
            status: "ok".into(),
            mission_id: Some(source.mission_id),
            revision: Some(source.revision),
            mission_state: None,
            publication_state: None,
            diagnostics: vec![],
            rendered: None,
            accepted: None,
            source: Some(source),
            migration_report: None,
            typed_migration: None,
            repair: None,
            output_record: None,
            external_status_snapshot: BTreeMap::new(),
        },
        Err(error) => failure(None, error),
    }
}
