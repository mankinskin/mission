use super::*;

pub(super) fn validate_handover(
    handover: &ExecutionHandoverV4,
    waypoint_id: &str,
    check_ids: &BTreeSet<&str>,
    postconditions: &[ExecutionConditionV4],
    ids: &mut BTreeSet<String>,
) -> Result<(), MissionError> {
    register_id(ids, &handover.id, waypoint_id, "/handover/id")?;
    validate_endpoint(
        &handover.producer,
        waypoint_id,
        Some(check_ids),
        "/handover/producer",
    )?;
    validate_check_condition_links(
        &handover.producer.check_ids,
        postconditions,
        waypoint_id,
        "/handover/producer/check_ids",
        "postcondition",
    )?;
    validate_endpoint(&handover.consumer, waypoint_id, None, "/handover/consumer")?;
    if handover.producer.artifact_id != handover.consumer.artifact_id
        || handover.producer.schema != handover.consumer.schema
    {
        return invalid(
            waypoint_id,
            "/handover",
            "producer and consumer must agree on artifact ID and schema.",
        );
    }
    register_id(
        ids,
        &handover.producer.artifact_id,
        waypoint_id,
        "/handover/artifact_id",
    )
}

fn validate_endpoint(
    endpoint: &ExecutionHandoverEndpointV4,
    waypoint_id: &str,
    check_ids: Option<&BTreeSet<&str>>,
    pointer: &str,
) -> Result<(), MissionError> {
    if !valid_contract_id(&endpoint.artifact_id) {
        return invalid(
            waypoint_id,
            pointer,
            "handover artifact IDs must be lowercase stable identifiers.",
        );
    }
    require_nonblank(&endpoint.waypoint_id, waypoint_id, pointer)?;
    require_nonblank(&endpoint.content_reference, waypoint_id, pointer)?;
    require_nonblank(&endpoint.schema, waypoint_id, pointer)?;
    validate_root(&endpoint.root, waypoint_id, pointer)?;
    if let Some(check_ids) = check_ids {
        validate_check_refs(&endpoint.check_ids, check_ids, waypoint_id, pointer)
    } else if endpoint.check_ids.is_empty() || has_duplicates(&endpoint.check_ids) {
        invalid(
            waypoint_id,
            pointer,
            "handover check references must be explicit and unique.",
        )
    } else {
        Ok(())
    }
}

pub(super) fn validate_endpoint_checks(
    ids: &[String],
    contract: &WaypointExecutionContractV4,
    pointer: &str,
    waypoint_id: &str,
) -> Result<(), MissionError> {
    let check_ids: BTreeSet<&str> = contract
        .checks
        .iter()
        .map(|check| check.id.as_str())
        .collect();
    validate_check_refs(ids, &check_ids, waypoint_id, pointer)
}

pub(super) fn validate_check_condition_links(
    check_ids: &[String],
    conditions: &[ExecutionConditionV4],
    waypoint_id: &str,
    pointer: &str,
    condition_kind: &str,
) -> Result<(), MissionError> {
    let linked_check_ids: BTreeSet<&str> = conditions
        .iter()
        .flat_map(|condition| condition.check_ids.iter().map(String::as_str))
        .collect();
    if check_ids
        .iter()
        .any(|check_id| !linked_check_ids.contains(check_id.as_str()))
    {
        return invalid(
            waypoint_id,
            pointer,
            &format!("handover checks must support a declared {condition_kind}."),
        );
    }
    Ok(())
}

pub(super) fn validate_tool_policy(
    policy: &ExecutionToolPolicyV4,
    boundary_paths: &[String],
    waypoint_id: &str,
) -> Result<(), MissionError> {
    if policy.actions.is_empty() || policy.permissions.is_empty() || policy.read_roots.is_empty() {
        return invalid(
            waypoint_id,
            "/tool_policy",
            "tool actions, permissions, and read roots must be explicit and nonempty.",
        );
    }
    if has_duplicates(&policy.actions) || has_duplicates(&policy.permissions) {
        return invalid(
            waypoint_id,
            "/tool_policy",
            "tool actions and permissions must be unique.",
        );
    }
    for root in policy.read_roots.iter().chain(&policy.write_roots) {
        validate_root(root, waypoint_id, "/tool_policy/roots")?;
    }
    for root in &policy.write_roots {
        if !boundary_paths.iter().any(|boundary| {
            boundary == "."
                || root == boundary
                || root
                    .strip_prefix(boundary)
                    .is_some_and(|remainder| remainder.starts_with('/'))
        }) {
            return invalid(
                waypoint_id,
                "/tool_policy/write_roots",
                "write roots must be equal to or nested within a declared task boundary.",
            );
        }
    }
    ensure_unique(&policy.read_roots, waypoint_id, "/tool_policy/read_roots")?;
    ensure_unique(&policy.write_roots, waypoint_id, "/tool_policy/write_roots")?;
    let can_read = policy.actions.iter().any(|action| {
        matches!(
            action,
            ExecutionToolActionV4::FilesystemRead | ExecutionToolActionV4::CliReadOnly
        )
    });
    let can_write = policy.actions.iter().any(|action| {
        matches!(
            action,
            ExecutionToolActionV4::FilesystemWrite | ExecutionToolActionV4::CliMutating
        )
    });
    let can_execute = policy.actions.iter().any(|action| {
        matches!(
            action,
            ExecutionToolActionV4::CliReadOnly | ExecutionToolActionV4::CliMutating
        )
    });
    if !can_read || !policy.permissions.contains(&ExecutionPermissionV4::Read) {
        return invalid(
            waypoint_id,
            "/tool_policy/read_roots",
            "read roots require an explicit read action and permission.",
        );
    }
    if policy.write_roots.is_empty() {
        require_reason(
            policy.no_write_reason.as_deref(),
            waypoint_id,
            "/tool_policy/no_write_reason",
        )?;
        if can_write || policy.permissions.contains(&ExecutionPermissionV4::Write) {
            return invalid(
                waypoint_id,
                "/tool_policy/write_roots",
                "write actions and permissions require declared write roots.",
            );
        }
    } else {
        if policy.no_write_reason.is_some()
            || !can_write
            || !policy.permissions.contains(&ExecutionPermissionV4::Write)
        {
            return invalid(
                waypoint_id,
                "/tool_policy/write_roots",
                "write roots require a write action and permission, without a no-write reason.",
            );
        }
    }
    if can_execute != policy.permissions.contains(&ExecutionPermissionV4::Execute) {
        return invalid(
            waypoint_id,
            "/tool_policy/permissions",
            "execute permission must exactly match whether a CLI action is declared.",
        );
    }
    Ok(())
}

pub(super) fn validate_check_refs(
    refs: &[String],
    known: &BTreeSet<&str>,
    waypoint_id: &str,
    pointer: &str,
) -> Result<(), MissionError> {
    if refs.is_empty() {
        return invalid(
            waypoint_id,
            pointer,
            "at least one check reference is required.",
        );
    }
    let mut unique = BTreeSet::new();
    if refs
        .iter()
        .any(|reference| !known.contains(reference.as_str()) || !unique.insert(reference))
    {
        return invalid(
            waypoint_id,
            pointer,
            "check references must be unique and identify declared checks.",
        );
    }
    Ok(())
}

pub(super) fn register_id(
    ids: &mut BTreeSet<String>,
    id: &str,
    waypoint_id: &str,
    pointer: &str,
) -> Result<(), MissionError> {
    if !valid_contract_id(id) || !ids.insert(id.to_owned()) {
        return invalid(
            waypoint_id,
            pointer,
            "contract IDs must be unique lowercase stable identifiers.",
        );
    }
    Ok(())
}

fn valid_contract_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value.as_bytes()[0].is_ascii_lowercase()
}

pub(super) fn validate_root(
    root: &str,
    waypoint_id: &str,
    pointer: &str,
) -> Result<(), MissionError> {
    let safe = root == "."
        || (!root.is_empty()
            && root.trim() == root
            && !root.starts_with('/')
            && !root.contains('\\')
            && !root.contains(':')
            && !root.chars().any(char::is_control)
            && root
                .split('/')
                .all(|component| !component.is_empty() && component != "." && component != ".."));
    if safe {
        Ok(())
    } else {
        invalid(
            waypoint_id,
            pointer,
            "roots must be normalized relative paths without empty, dot, traversal, absolute, or platform-specific components.",
        )
    }
}

pub(super) fn ensure_unique<T: Ord>(
    values: &[T],
    waypoint_id: &str,
    pointer: &str,
) -> Result<(), MissionError> {
    if has_duplicates(values) {
        invalid(waypoint_id, pointer, "values must be unique.")
    } else {
        Ok(())
    }
}

fn has_duplicates<T: Ord>(values: &[T]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() != values.len()
}

pub(super) fn require_nonblank(
    value: &str,
    waypoint_id: &str,
    pointer: &str,
) -> Result<(), MissionError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        invalid(
            waypoint_id,
            pointer,
            "required values must be nonblank and contain no control characters.",
        )
    } else {
        Ok(())
    }
}

pub(super) fn require_reason(
    reason: Option<&str>,
    waypoint_id: &str,
    pointer: &str,
) -> Result<(), MissionError> {
    match reason {
        Some(value) => require_nonblank(value, waypoint_id, pointer),
        None => invalid(
            waypoint_id,
            pointer,
            "an omitted optional route requires an explicit reason.",
        ),
    }
}

pub(super) fn invalid(waypoint_id: &str, pointer: &str, message: &str) -> Result<(), MissionError> {
    Err(execution_contract_error(
        "invalid-execution-contract",
        pointer,
        waypoint_id,
        message,
    ))
}

pub(super) fn execution_contract_error(
    code: &'static str,
    pointer: &str,
    waypoint_id: &str,
    message: &str,
) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "waypoint".into(),
            pointer: pointer.into(),
            waypoint_id: Some(waypoint_id.into()),
            message: message.into(),
        }],
    )
}
