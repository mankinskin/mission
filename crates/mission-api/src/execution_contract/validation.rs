use super::validation_details::*;
use super::*;

pub(crate) fn validate_contract_set(waypoints: &[TypedWaypoint]) -> Result<(), MissionError> {
    let known_ids: BTreeSet<&str> = waypoints
        .iter()
        .map(|waypoint| waypoint.waypoint_id.as_str())
        .collect();
    let mut by_id = BTreeMap::new();
    for waypoint in waypoints {
        let contract = waypoint.execution_contract.as_ref().ok_or_else(|| {
            execution_contract_error(
                "execution-contract-required",
                "/execution_contract",
                &waypoint.waypoint_id,
                "schema v4 requires one complete execution contract per waypoint.",
            )
        })?;
        if waypoint.work_kind != Some(contract.work_kind) {
            return Err(execution_contract_error(
                "execution-contract-work-kind-mismatch",
                "/work_kind",
                &waypoint.waypoint_id,
                "TypedWaypoint.work_kind must match the independent contract work_kind.",
            ));
        }
        validate_contract(contract, waypoint, &known_ids)?;
        by_id.insert(waypoint.waypoint_id.as_str(), waypoint);
    }

    validate_dependency_graph(waypoints)?;
    for waypoint in waypoints {
        let contract = waypoint
            .execution_contract
            .as_ref()
            .expect("required above");
        let Some(handover) = &contract.handover else {
            continue;
        };
        let producer = &handover.producer;
        let consumer = &handover.consumer;
        if producer.waypoint_id != waypoint.waypoint_id {
            return Err(execution_contract_error(
                "execution-handover-producer-mismatch",
                "/handover/producer/waypoint_id",
                &waypoint.waypoint_id,
                "the handover producer must be the waypoint owning this contract.",
            ));
        }
        let consumer_waypoint = by_id.get(consumer.waypoint_id.as_str()).ok_or_else(|| {
            execution_contract_error(
                "execution-handover-unknown-consumer",
                "/handover/consumer/waypoint_id",
                &waypoint.waypoint_id,
                "the handover consumer must reference a declared waypoint.",
            )
        })?;
        if !consumer_waypoint
            .depends_on
            .iter()
            .any(|dependency| dependency == &waypoint.waypoint_id)
        {
            return Err(execution_contract_error(
                "execution-handover-dependency-mismatch",
                "/handover/consumer/waypoint_id",
                &waypoint.waypoint_id,
                "the consumer must declare the producer as a prerequisite.",
            ));
        }
        let consumer_contract = consumer_waypoint
            .execution_contract
            .as_ref()
            .expect("all schema-v4 waypoints have a contract");
        validate_endpoint_checks(
            &consumer.check_ids,
            consumer_contract,
            "/handover/consumer/check_ids",
            &waypoint.waypoint_id,
        )?;
        validate_check_condition_links(
            &consumer.check_ids,
            &consumer_contract.preconditions,
            &waypoint.waypoint_id,
            "/handover/consumer/check_ids",
            "precondition",
        )?;
    }
    Ok(())
}

pub(crate) fn validate_bundle_contracts(bundle: &MissionBundle) -> Result<(), Vec<Diagnostic>> {
    let waypoints: Vec<TypedWaypoint> = bundle
        .waypoints
        .iter()
        .map(|fragment| {
            let mut waypoint = TypedWaypoint {
                waypoint_id: fragment.waypoint_id.clone(),
                depends_on: fragment.depends_on.clone(),
                execution_contract: fragment.execution_contract.clone(),
                ..TypedWaypoint::default()
            };
            waypoint.work_kind = fragment
                .execution_contract
                .as_ref()
                .map(|contract| contract.work_kind);
            waypoint
        })
        .collect();
    validate_contract_set(&waypoints).map_err(|error| {
        error
            .diagnostics()
            .map_or_else(Vec::new, |diagnostics| diagnostics.to_vec())
    })
}

fn validate_dependency_graph(waypoints: &[TypedWaypoint]) -> Result<(), MissionError> {
    let mut remaining: BTreeMap<&str, usize> = waypoints
        .iter()
        .map(|waypoint| (waypoint.waypoint_id.as_str(), waypoint.depends_on.len()))
        .collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for waypoint in waypoints {
        for dependency in &waypoint.depends_on {
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(waypoint.waypoint_id.as_str());
        }
    }
    let mut ready: Vec<&str> = remaining
        .iter()
        .filter_map(|(id, count)| (*count == 0).then_some(*id))
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop() {
        visited += 1;
        for dependent in dependents.get(id).into_iter().flatten() {
            let count = remaining.get_mut(dependent).expect("known dependent");
            *count -= 1;
            if *count == 0 {
                ready.push(dependent);
            }
        }
    }
    if visited == waypoints.len() {
        return Ok(());
    }
    let id = remaining
        .iter()
        .find_map(|(id, count)| (*count > 0).then_some(*id))
        .unwrap_or("unknown");
    Err(execution_contract_error(
        "execution-dependency-cycle",
        "/declared_dependency_ids",
        id,
        "declared waypoint dependencies must form an acyclic graph.",
    ))
}

fn validate_contract(
    contract: &WaypointExecutionContractV4,
    waypoint: &TypedWaypoint,
    known_ids: &BTreeSet<&str>,
) -> Result<(), MissionError> {
    let id = waypoint.waypoint_id.as_str();
    if contract.schema_version != EXECUTION_CONTRACT_SCHEMA_VERSION {
        return invalid(
            id,
            "/schema_version",
            "only execution contract schema v4 is supported.",
        );
    }
    for (pointer, value) in [
        ("/expected_outcome", contract.expected_outcome.as_str()),
        ("/rationale", contract.rationale.as_str()),
        ("/boundary/task", contract.boundary.task.as_str()),
        (
            "/environment/profile",
            contract.environment.profile.as_str(),
        ),
        (
            "/environment/description",
            contract.environment.description.as_str(),
        ),
        (
            "/failure_policy/trigger",
            contract.failure_policy.trigger.as_str(),
        ),
        (
            "/failure_policy/diagnostic_action",
            contract.failure_policy.diagnostic_action.as_str(),
        ),
        (
            "/failure_policy/escalation_target",
            contract.failure_policy.escalation_target.as_str(),
        ),
    ] {
        require_nonblank(value, id, pointer)?;
    }
    if contract.authoritative_context.is_empty() {
        return invalid(
            id,
            "/authoritative_context",
            "at least one authoritative context reference is required.",
        );
    }
    if contract.boundary.paths.is_empty() {
        return invalid(
            id,
            "/boundary/paths",
            "at least one bounded repository path is required.",
        );
    }
    let mut ids = BTreeSet::new();
    for context in &contract.authoritative_context {
        register_id(&mut ids, &context.id, id, "/authoritative_context/id")?;
        require_nonblank(&context.reference, id, "/authoritative_context/reference")?;
        require_nonblank(
            &context.description,
            id,
            "/authoritative_context/description",
        )?;
    }
    for path in &contract.boundary.paths {
        validate_root(path, id, "/boundary/paths")?;
    }
    ensure_unique(&contract.boundary.paths, id, "/boundary/paths")?;

    let dependencies = &contract.declared_dependency_ids;
    if dependencies.is_empty() {
        require_reason(
            contract.no_dependencies_reason.as_deref(),
            id,
            "/no_dependencies_reason",
        )?;
    } else {
        if contract.no_dependencies_reason.is_some() {
            return invalid(
                id,
                "/no_dependencies_reason",
                "a dependency reason is valid only when no dependencies are declared.",
            );
        }
        let mut unique = BTreeSet::new();
        for dependency in dependencies {
            if dependency == id
                || !known_ids.contains(dependency.as_str())
                || !unique.insert(dependency)
            {
                return invalid(
                    id,
                    "/declared_dependency_ids",
                    "dependencies must be unique references to other declared waypoints.",
                );
            }
        }
    }
    let source_dependencies: BTreeSet<&str> =
        waypoint.depends_on.iter().map(String::as_str).collect();
    let contract_dependencies: BTreeSet<&str> = dependencies.iter().map(String::as_str).collect();
    if source_dependencies != contract_dependencies {
        return invalid(
            id,
            "/declared_dependency_ids",
            "contract dependencies must match the typed waypoint dependencies exactly.",
        );
    }

    if contract.preconditions.is_empty() || contract.postconditions.is_empty() {
        return invalid(
            id,
            "/preconditions, /postconditions",
            "at least one independently checkable precondition and postcondition is required.",
        );
    }
    let mut check_ids = BTreeSet::new();
    for check in &contract.checks {
        register_id(&mut ids, &check.id, id, "/checks/id")?;
        check_ids.insert(check.id.as_str());
        require_nonblank(&check.command, id, "/checks/command")?;
        validate_root(&check.cwd, id, "/checks/cwd")?;
    }
    if !contract
        .checks
        .iter()
        .any(|check| check.kind == ExecutionCheckKindV4::Acceptance)
        || !contract
            .checks
            .iter()
            .any(|check| check.kind == ExecutionCheckKindV4::Preflight)
    {
        return invalid(
            id,
            "/checks",
            "at least one acceptance check and one preflight check are required.",
        );
    }
    for conditions in [&contract.preconditions, &contract.postconditions] {
        for condition in conditions {
            register_id(&mut ids, &condition.id, id, "/conditions/id")?;
            require_nonblank(&condition.description, id, "/conditions/description")?;
            validate_check_refs(
                &condition.check_ids,
                &check_ids,
                id,
                "/conditions/check_ids",
            )?;
        }
    }
    if contract.evidence_requirements.is_empty() {
        return invalid(
            id,
            "/evidence_requirements",
            "at least one evidence requirement is required.",
        );
    }
    for evidence in &contract.evidence_requirements {
        register_id(&mut ids, &evidence.id, id, "/evidence_requirements/id")?;
        validate_check_refs(
            &evidence.check_ids,
            &check_ids,
            id,
            "/evidence_requirements/check_ids",
        )?;
        if evidence.check_ids.iter().any(|check_id| {
            contract
                .checks
                .iter()
                .find(|check| check.id == *check_id)
                .is_some_and(|check| check.evidence_class != evidence.class)
        }) {
            return invalid(
                id,
                "/evidence_requirements/class",
                "evidence class must match every referenced check.",
            );
        }
    }
    let regression_ids: BTreeSet<&str> = contract
        .checks
        .iter()
        .filter(|check| check.kind == ExecutionCheckKindV4::Regression)
        .map(|check| check.id.as_str())
        .collect();
    if contract.regression_check_ids.is_empty() {
        require_reason(
            contract.no_regression_checks_reason.as_deref(),
            id,
            "/no_regression_checks_reason",
        )?;
    } else {
        if contract.no_regression_checks_reason.is_some() {
            return invalid(
                id,
                "/no_regression_checks_reason",
                "a no-regression reason is valid only when no regression checks are declared.",
            );
        }
        validate_check_refs(
            &contract.regression_check_ids,
            &check_ids,
            id,
            "/regression_check_ids",
        )?;
        if contract
            .regression_check_ids
            .iter()
            .any(|check_id| !regression_ids.contains(check_id.as_str()))
        {
            return invalid(
                id,
                "/regression_check_ids",
                "regression_check_ids must reference checks classified as regression.",
            );
        }
    }

    validate_tool_policy(&contract.tool_policy, &contract.boundary.paths, id)?;
    if contract.environment.preflight_check_ids.is_empty() {
        return invalid(
            id,
            "/environment/preflight_check_ids",
            "the environment must declare at least one preflight check.",
        );
    }
    validate_check_refs(
        &contract.environment.preflight_check_ids,
        &check_ids,
        id,
        "/environment/preflight_check_ids",
    )?;
    if contract
        .environment
        .preflight_check_ids
        .iter()
        .any(|check_id| {
            contract
                .checks
                .iter()
                .find(|check| check.id == *check_id)
                .is_some_and(|check| check.kind != ExecutionCheckKindV4::Preflight)
        })
    {
        return invalid(
            id,
            "/environment/preflight_check_ids",
            "preflight references must identify checks classified as preflight.",
        );
    }
    if contract.failure_policy.max_attempts == 0 {
        return invalid(
            id,
            "/failure_policy/max_attempts",
            "failure handling must have a finite positive attempt count.",
        );
    }

    match (&contract.handover, &contract.no_handover_reason) {
        (Some(handover), None) => {
            validate_handover(handover, id, &check_ids, &contract.postconditions, &mut ids)?
        }
        (None, Some(reason)) if !reason.trim().is_empty() => {}
        (None, _) => {
            return invalid(
                id,
                "/no_handover_reason",
                "an absent handover requires an explicit reason.",
            );
        }
        (Some(_), Some(_)) => {
            return invalid(
                id,
                "/no_handover_reason",
                "a declared handover and no-handover reason are mutually exclusive.",
            );
        }
    }
    match (&contract.provider_route, &contract.no_provider_reason) {
        (Some(provider), None) => {
            register_id(&mut ids, &provider.id, id, "/provider_route/id")?;
            for value in [
                &provider.provider,
                &provider.version,
                &provider.receipt_class,
            ] {
                require_nonblank(value, id, "/provider_route")?;
            }
            validate_check_refs(
                &provider.preflight_check_ids,
                &check_ids,
                id,
                "/provider_route/preflight_check_ids",
            )?;
            if provider.preflight_check_ids.is_empty()
                || provider.preflight_check_ids.iter().any(|check_id| {
                    contract
                        .checks
                        .iter()
                        .find(|check| check.id == *check_id)
                        .is_some_and(|check| check.kind != ExecutionCheckKindV4::Preflight)
                })
            {
                return invalid(
                    id,
                    "/provider_route/preflight_check_ids",
                    "provider preflight references must name declared preflight checks.",
                );
            }
        }
        (None, Some(reason)) if !reason.trim().is_empty() => {}
        (None, _) => {
            return invalid(
                id,
                "/no_provider_reason",
                "an absent provider route requires an explicit reason.",
            );
        }
        (Some(_), Some(_)) => {
            return invalid(
                id,
                "/no_provider_reason",
                "a provider route and no-provider reason are mutually exclusive.",
            );
        }
    }
    Ok(())
}
