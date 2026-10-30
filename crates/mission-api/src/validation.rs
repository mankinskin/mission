use super::*;

pub fn collect_bundle(manifest_path: &Path) -> Result<MissionBundle, MissionError> {
    let manifest_bytes = fs::read(manifest_path)?;
    let manifest: BundleManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        MissionError::Validation(
            1,
            vec![diag(
                "invalid-manifest-json",
                manifest_path,
                "",
                None,
                &format!("Manifest must be valid strict JSON: {error}"),
            )],
        )
    })?;
    let root = manifest_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let mut diagnostics = validate_manifest_shape(&manifest, manifest_path);
    let mut waypoints = Vec::new();
    let mut seen_paths = BTreeSet::new();
    for (index, fragment) in manifest.waypoint_files.iter().enumerate() {
        let pointer = format!("/waypoint_files/{index}");
        if !seen_paths.insert(fragment) {
            diagnostics.push(diag(
                "duplicate-fragment-path",
                manifest_path,
                &pointer,
                None,
                "Each waypoint fragment path must be listed once.",
            ));
            continue;
        }
        let Some(path) = safe_fragment_path(&root, fragment) else {
            diagnostics.push(diag("unsafe-fragment-path", manifest_path, &pointer, None, "Use a relative path inside the bundle root; absolute, traversal, and symlink escapes are forbidden."));
            continue;
        };
        match fs::read(&path)
            .and_then(|bytes| serde_json::from_slice(&bytes).map_err(std::io::Error::other))
        {
            Ok(waypoint) => waypoints.push(waypoint),
            Err(error) => diagnostics.push(diag(
                "fragment-read-failed",
                &path,
                "",
                None,
                &format!("Waypoint fragment cannot be read as strict JSON: {error}"),
            )),
        }
    }
    if diagnostics.is_empty() {
        let bundle = MissionBundle {
            manifest,
            waypoints,
        };
        validate_bundle(&bundle)
            .map_err(|diagnostics| MissionError::Validation(diagnostics.len(), diagnostics))?;
        Ok(bundle)
    } else {
        Err(MissionError::Validation(diagnostics.len(), diagnostics))
    }
}

pub fn validate_bundle(bundle: &MissionBundle) -> Result<(), Vec<Diagnostic>> {
    let mut diagnostics = validate_manifest_shape(&bundle.manifest, Path::new("manifest.json"));
    diagnostics.extend(schema_renderers::validate_requirement_shapes(
        &bundle.manifest.requirements,
    ));
    let mut ids = BTreeSet::new();
    let mut waypoint_by_id = BTreeMap::new();
    for (index, waypoint) in bundle.waypoints.iter().enumerate() {
        if let Err(diagnostic) = outcome_plan::validate_outcome_plan(
            waypoint.outcome_plan.as_ref(),
            bundle.manifest.schema_version == OUTCOME_BUNDLE_SCHEMA_VERSION,
            &waypoint.waypoint_id,
        ) {
            diagnostics.push(diagnostic);
        }
        if let Err(diagnostic) = waypoint.relationships.validate() {
            diagnostics.push(diagnostic);
        }
        let source = format!("waypoint[{index}]");
        if !valid_stable_id(&waypoint.waypoint_id) {
            diagnostics.push(diag(
                "invalid-waypoint-id",
                Path::new(&source),
                "/waypoint_id",
                Some(&waypoint.waypoint_id),
                "waypoint_id must be a lowercase slug containing only letters, digits, and hyphens.",
            ));
        } else if !ids.insert(&waypoint.waypoint_id) {
            diagnostics.push(diag(
                "duplicate-waypoint-id",
                Path::new(&source),
                "/waypoint_id",
                Some(&waypoint.waypoint_id),
                "Every waypoint_id must be unique.",
            ));
        }
        if waypoint.title.trim().is_empty()
            || waypoint.session_package.trim().is_empty()
            || waypoint.prompt.trim().is_empty()
            || waypoint.validation.is_empty()
            || waypoint
                .validation
                .iter()
                .any(|gate| gate.command.trim().is_empty())
            || waypoint.commit_checkpoint.message.trim().is_empty()
        {
            diagnostics.push(diag(
                "missing-required-content",
                Path::new(&source),
                "/prompt",
                Some(&waypoint.waypoint_id),
                "Waypoints require a title, session package, prompt, validation command, and commit checkpoint.",
            ));
        }
        let ticket_backed =
            matches!(waypoint.scope, Scope::TicketBacked { .. }) || waypoint.ticket_ref.is_some();
        if ticket_backed && waypoint.state.is_some() {
            diagnostics.push(diag(
                "ticket-state-owned-externally",
                Path::new(&source),
                "/state",
                Some(&waypoint.waypoint_id),
                "Ticket-backed waypoints must not carry local mutable state.",
            ));
        }
        if let Some(ticket_ref) = &waypoint.ticket_ref
            && !canonical_reference(ticket_ref, "ticket")
        {
            diagnostics.push(diag(
                "invalid-canonical-reference",
                Path::new(&source),
                "/ticket_ref",
                Some(&waypoint.waypoint_id),
                "Ticket references must use ce://<workspace>/ticket/<uuid>.",
            ));
        }
        if let (
            Some(ticket_ref),
            Scope::TicketBacked {
                ticket_ref: scoped_ref,
            },
        ) = (&waypoint.ticket_ref, &waypoint.scope)
            && ticket_ref != scoped_ref
        {
            diagnostics.push(diag(
                "ticket-reference-mismatch",
                Path::new(&source),
                "/ticket_ref",
                Some(&waypoint.waypoint_id),
                "ticket_ref must match the reference declared by ticket-backed scope.",
            ));
        }
        if let Some(child_mission_ref) = &waypoint.child_mission_ref {
            if !canonical_reference(child_mission_ref, "mission") {
                diagnostics.push(diag(
                    "invalid-child-mission-reference",
                    Path::new(&source),
                    "/child_mission_ref",
                    Some(&waypoint.waypoint_id),
                    "Child mission references must use ce://<workspace>/mission/<uuid>.",
                ));
            }
            if waypoint.state.is_some() {
                diagnostics.push(diag(
                    "child-mission-state-derived",
                    Path::new(&source),
                    "/state",
                    Some(&waypoint.waypoint_id),
                    "Child mission waypoint state is derived from the accepted child mission.",
                ));
            }
        }
        if let Scope::TicketBacked { ticket_ref } = &waypoint.scope
            && !canonical_reference(ticket_ref, "ticket")
        {
            diagnostics.push(diag(
                "invalid-canonical-reference",
                Path::new(&source),
                "/scope/ticket_ref",
                Some(&waypoint.waypoint_id),
                "Ticket references must use ce://<workspace>/ticket/<uuid>.",
            ));
        }
        waypoint_by_id.insert(waypoint.waypoint_id.as_str(), waypoint);
        let mut dependencies = BTreeSet::new();
        for (dependency_index, dependency) in waypoint.depends_on.iter().enumerate() {
            if !dependencies.insert(dependency) {
                diagnostics.push(diag(
                    "duplicate-prerequisite",
                    Path::new(&source),
                    &format!("/depends_on/{dependency_index}"),
                    Some(&waypoint.waypoint_id),
                    "A waypoint may declare each prerequisite only once.",
                ));
            }
        }
    }

    diagnostics.extend(operator_results::validate(bundle));
    let mut requirement_ids = BTreeSet::new();
    for (index, requirement) in bundle.manifest.requirements.iter().enumerate() {
        if !valid_stable_id(&requirement.id) {
            diagnostics.push(diag(
                "invalid-requirement-id",
                Path::new("manifest.json"),
                &format!("/requirements/{index}/id"),
                None,
                "Requirement IDs must be lowercase slugs containing only letters, digits, and hyphens.",
            ));
        } else if !requirement_ids.insert(&requirement.id) {
            diagnostics.push(diag(
                "duplicate-requirement-id",
                Path::new("manifest.json"),
                &format!("/requirements/{index}/id"),
                None,
                "Requirement IDs must be unique.",
            ));
        }
        if requirement.text.trim().is_empty() {
            diagnostics.push(diag(
                "missing-required-content",
                Path::new("manifest.json"),
                &format!("/requirements/{index}/text"),
                None,
                "Requirement text must not be empty.",
            ));
        }
    }
    let order = &bundle.manifest.execution_order;
    if order.is_empty() {
        diagnostics.push(diag(
            "empty-mission",
            Path::new("manifest.json"),
            "/execution_order",
            None,
            "An accepted mission must contain at least one waypoint.",
        ));
    }
    if order.len() != waypoint_by_id.len()
        || order.iter().collect::<BTreeSet<_>>().len() != order.len()
        || order
            .iter()
            .any(|id| !waypoint_by_id.contains_key(id.as_str()))
    {
        diagnostics.push(diag(
            "order-requires-each-waypoint-once",
            Path::new("manifest.json"),
            "/execution_order",
            None,
            "execution_order must contain every known waypoint_id exactly once.",
        ));
    }
    let positions: BTreeMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for waypoint in &bundle.waypoints {
        for prerequisite in &waypoint.depends_on {
            match (
                positions.get(prerequisite.as_str()),
                positions.get(waypoint.waypoint_id.as_str()),
            ) {
                (Some(before), Some(after)) if before < after => {}
                (Some(_), Some(_)) => diagnostics.push(diag(
                    "prerequisite-must-precede-dependent",
                    Path::new("manifest.json"),
                    "/execution_order",
                    Some(&waypoint.waypoint_id),
                    "Every prerequisite must occur before its dependent in execution_order.",
                )),
                _ => diagnostics.push(diag(
                    "unknown-prerequisite",
                    Path::new("manifest.json"),
                    "/execution_order",
                    Some(&waypoint.waypoint_id),
                    "Every prerequisite must name a waypoint in this bundle.",
                )),
            }
        }
    }
    for (index, id) in order.iter().enumerate().skip(1) {
        if let Some(waypoint) = waypoint_by_id.get(id.as_str()) {
            let predecessor = &order[index - 1];
            if !waypoint
                .depends_on
                .iter()
                .any(|dependency| dependency == predecessor)
            {
                diagnostics.push(diag("missing-immediate-predecessor", Path::new("manifest.json"), "/execution_order", Some(id), "Each waypoint after the first must explicitly depend on its immediate predecessor checkpoint."));
            }
        }
    }
    let requirements: BTreeSet<&str> = bundle
        .manifest
        .requirements
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    for waypoint in &bundle.waypoints {
        for id in &waypoint.requirement_ids {
            if !requirements.contains(id.as_str()) {
                diagnostics.push(diag(
                    "unknown-requirement",
                    Path::new("manifest.json"),
                    "/requirements",
                    Some(&waypoint.waypoint_id),
                    "Waypoint requirement_ids must refer to a manifest requirement.",
                ));
            }
        }
    }
    for requirement in &bundle.manifest.requirements {
        if !requirement.deferred
            && !bundle
                .waypoints
                .iter()
                .any(|w| w.requirement_ids.contains(&requirement.id))
        {
            diagnostics.push(diag(
                "unowned-requirement",
                Path::new("manifest.json"),
                "/requirements",
                None,
                "Each non-deferred requirement must be owned by a waypoint.",
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

pub(crate) fn validate_manifest_shape(manifest: &BundleManifest, path: &Path) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if !matches!(
        manifest.schema_version,
        SCHEMA_VERSION | OUTCOME_BUNDLE_SCHEMA_VERSION
    ) {
        diagnostics.push(diag(
            "unsupported-schema-version",
            path,
            "/schema_version",
            None,
            "Only native schema_version 1 (legacy) and 2 (planned outcomes) are supported.",
        ));
    }
    if manifest.revision == 0
        || manifest.title.trim().is_empty()
        || manifest.objective.trim().is_empty()
    {
        diagnostics.push(diag(
            "missing-required-content",
            path,
            "",
            None,
            "revision, title, and objective must be nonempty valid values.",
        ));
    }
    diagnostics
}

fn valid_stable_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && value
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && value
            .as_bytes()
            .last()
            .is_some_and(u8::is_ascii_alphanumeric)
}

fn safe_fragment_path(root: &Path, value: &str) -> Option<PathBuf> {
    let candidate = Path::new(value);
    if candidate.is_absolute()
        || candidate.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return None;
    }
    let canonical = root.join(candidate).canonicalize().ok()?;
    canonical.starts_with(root).then_some(canonical)
}

fn canonical_reference(value: &str, store: &str) -> bool {
    let Some(path) = value.strip_prefix("ce://") else {
        return false;
    };
    let mut segments = path.split('/');
    let (Some(workspace), Some(actual_store), Some(id), None) = (
        segments.next(),
        segments.next(),
        segments.next(),
        segments.next(),
    ) else {
        return false;
    };
    !workspace.is_empty()
        && !workspace
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte == b'\\')
        && actual_store == store
        && Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id)
}

pub(crate) fn mission_reference(value: &str) -> Option<(&str, Uuid)> {
    let path = value.strip_prefix("ce://")?;
    let mut segments = path.split('/');
    let (workspace, store, id, None) = (
        segments.next()?,
        segments.next()?,
        segments.next()?,
        segments.next(),
    ) else {
        return None;
    };
    (store == "mission").then_some((workspace, Uuid::parse_str(id).ok()?))
}

pub(crate) fn mission_reference_id(value: &str) -> Option<Uuid> {
    mission_reference(value).map(|(_, id)| id)
}

pub(crate) fn first_incomplete_state(bundle: &MissionBundle) -> WaypointState {
    bundle
        .manifest
        .execution_order
        .iter()
        .find_map(|id| {
            bundle
                .waypoints
                .iter()
                .find(|waypoint| waypoint.waypoint_id == *id)
                .map(|waypoint| waypoint.state.clone().unwrap_or(WaypointState::Pending))
                .filter(|state| *state != WaypointState::Completed)
        })
        .unwrap_or(WaypointState::Completed)
}

pub(crate) fn diag(
    code: &'static str,
    source: &Path,
    pointer: &str,
    waypoint_id: Option<&str>,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        code,
        source: source.display().to_string(),
        pointer: pointer.to_string(),
        waypoint_id: waypoint_id.map(str::to_owned),
        message: message.to_string(),
    }
}
