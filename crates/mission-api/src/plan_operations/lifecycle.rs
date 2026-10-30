use super::*;

impl MissionLifecycle {
    /// Read one coherent authoritative revision without writing or repairing files.
    pub fn review_plan(&self, mission_id: Uuid) -> Result<ReviewPlan, MissionError> {
        let mut attempt = 0;
        loop {
            match self.review_plan_once(mission_id) {
                Err(error) if attempt < SNAPSHOT_RETRIES && snapshot_retryable(&error) => {
                    attempt += 1;
                    std::thread::sleep(std::time::Duration::from_millis(25));
                }
                other => return other,
            }
        }
    }

    fn review_plan_once(&self, mission_id: Uuid) -> Result<ReviewPlan, MissionError> {
        // Preserve NotFound for absent Missions; never read existing ones unguarded.
        self.store.get(mission_id)?;
        let _guard =
            typed_source::TypedLock::acquire_shared_existing(&self.store.root, mission_id)?;
        self.review_plan_unlocked(mission_id)
    }

    pub(crate) fn review_plan_unlocked(
        &self,
        mission_id: Uuid,
    ) -> Result<ReviewPlan, MissionError> {
        let first = self.store.get(mission_id)?;
        let source = self.get_mission_source(mission_id)?;
        let ledger = read_ledger(&self.mission_directory(mission_id))?;
        let second = self.store.get(mission_id)?;
        let revision = first.bundle.manifest.revision;
        if second.bundle.manifest.revision != revision
            || source.revision != revision
            || ledger.entries.iter().any(|entry| entry.revision > revision)
            || ledger.nonsemantic_revisions.iter().any(|r| *r > revision)
        {
            return Err(plan_error(
                "projection_diverged",
                "authority changed while the review plan was being read.",
                None,
            ));
        }
        build_review_plan(&first, &source, &ledger)
    }

    pub(crate) fn semantic_revision_unlocked(&self, mission_id: Uuid) -> Result<u64, MissionError> {
        let revision = self.store.get(mission_id)?.bundle.manifest.revision;
        let ledger = read_ledger(&self.mission_directory(mission_id))?;
        let evidence: std::collections::BTreeSet<u64> = ledger
            .entries
            .iter()
            .map(|entry| entry.revision)
            .chain(ledger.nonsemantic_revisions.iter().copied())
            .collect();
        Ok((1..=revision)
            .rev()
            .find(|candidate| !evidence.contains(candidate))
            .unwrap_or(revision))
    }

    /// Apply one CAS/idempotent mutation under one lock and one atomic publication.
    pub fn apply_plan_operation(
        &self,
        mission_id: Uuid,
        request: PlanOperationRequest,
    ) -> Result<PlanOperationOutcome, MissionError> {
        validate_request_id(&request.request_id)?;
        let request_sha = sha(&serde_json::to_vec(&(
            request.expected_revision,
            &request.operation,
        ))?);
        self.store.prepare_write()?;
        let _lock = typed_source::TypedLock::acquire(&self.store.root, mission_id)?;
        let directory = self.mission_directory(mission_id);
        if let Some(record) = read_record(&directory, mission_id, &request.request_id)? {
            if record.request_sha256 != request_sha {
                return Err(plan_error(
                    "request_id_conflict",
                    "request_id was already used for different content.",
                    None,
                ));
            }
            return Ok(PlanOperationOutcome {
                revision: record.revision,
                plan_digest: record.plan_digest,
                replayed: true,
            });
        }
        let source = self.get_mission_source(mission_id)?;
        let accepted = self.store.get(mission_id)?;
        let actual = accepted.bundle.manifest.revision;
        if actual != request.expected_revision {
            return Err(MissionError::StaleRevision {
                expected: request.expected_revision,
                actual,
            });
        }
        let ledger = read_ledger(&directory)?;
        let mut draft = source.clone();
        #[allow(clippy::type_complexity)]
        let mut adjust: Option<
            Box<dyn Fn(&mut MissionBundle) -> Result<(), MissionError>>,
        > = None;
        let mut new_ledger = ledger.clone();
        match request.operation.clone() {
            PlanOperation::SetGoal { title, objective } => {
                if title.trim().is_empty() || objective.trim().is_empty() {
                    return Err(plan_error(
                        "invalid_goal",
                        "title and objective must be nonblank.",
                        None,
                    ));
                }
                draft.title = title;
                draft.objective = objective;
            }
            PlanOperation::CreateWaypoint { waypoint, after } => {
                let at = match after {
                    None => draft.waypoints.len(),
                    Some(id) => index_of(&draft, &id)? + 1,
                };
                draft.waypoints.insert(at, waypoint);
            }
            PlanOperation::UpdateWaypoint { waypoint } => {
                let at = index_of(&draft, &waypoint.waypoint_id)?;
                draft.waypoints[at] = waypoint;
            }
            PlanOperation::RemoveWaypoint {
                waypoint_id,
                replacement,
            } => {
                let at = index_of(&draft, &waypoint_id)?;
                let retained = accepted.bundle.waypoints.iter().any(|fragment| {
                    fragment.waypoint_id == waypoint_id
                        && fragment.state == Some(WaypointState::Completed)
                }) || accepted
                    .bundle
                    .manifest
                    .operator_results
                    .iter()
                    .any(|result| result.waypoint_id == waypoint_id);
                if retained {
                    return Err(plan_error(
                        "retained-waypoint-required",
                        "completed or result-bearing waypoints are retained history.",
                        Some(&waypoint_id),
                    ));
                }
                match replacement {
                    Some(replacement) => {
                        if replacement.waypoint_id == waypoint_id {
                            return Err(plan_error(
                                "invalid_replacement",
                                "a replacement must have a new stable ID; use update to edit in place.",
                                Some(&waypoint_id),
                            ));
                        }
                        let inherited = accepted
                            .bundle
                            .waypoints
                            .iter()
                            .find(|fragment| fragment.waypoint_id == waypoint_id)
                            .map(|fragment| fragment.requirement_ids.clone())
                            .unwrap_or_default();
                        let new_id = replacement.waypoint_id.clone();
                        adjust = Some(Box::new(move |bundle: &mut MissionBundle| {
                            if let Some(fragment) = bundle
                                .waypoints
                                .iter_mut()
                                .find(|fragment| fragment.waypoint_id == new_id)
                            {
                                fragment.requirement_ids = inherited.clone();
                            }
                            Ok(())
                        }));
                        draft.waypoints[at] = replacement;
                    }
                    None => {
                        if draft
                            .waypoints
                            .iter()
                            .any(|other| other.depends_on.contains(&waypoint_id))
                        {
                            return Err(plan_error(
                                "replacement_required",
                                "other waypoints depend on this one; supply a valid replacement in the same operation.",
                                Some(&waypoint_id),
                            ));
                        }
                        draft.waypoints.remove(at);
                    }
                }
            }
            PlanOperation::ReorderWaypoints { order } => {
                for id in &order {
                    index_of(&draft, id)?;
                }
                let unique: BTreeSet<&String> = order.iter().collect();
                if order.len() != draft.waypoints.len() || unique.len() != order.len() {
                    return Err(plan_error(
                        "invalid_order",
                        "order must list every waypoint exactly once.",
                        None,
                    ));
                }
                let mut remaining = std::mem::take(&mut draft.waypoints);
                for id in &order {
                    let at = remaining
                        .iter()
                        .position(|waypoint| &waypoint.waypoint_id == id)
                        .ok_or_else(|| not_found(id))?;
                    draft.waypoints.push(remaining.remove(at));
                }
            }
            PlanOperation::SetRequirements { requirements } => {
                for requirement in &requirements {
                    for owner in &requirement.owner_waypoint_ids {
                        index_of(&draft, owner)?;
                    }
                }
                adjust = Some(Box::new(move |bundle: &mut MissionBundle| {
                    bundle.manifest.requirements = requirements
                        .iter()
                        .map(|item| item.requirement.clone())
                        .collect();
                    for fragment in &mut bundle.waypoints {
                        fragment.requirement_ids = requirements
                            .iter()
                            .filter(|item| item.owner_waypoint_ids.contains(&fragment.waypoint_id))
                            .map(|item| item.requirement.id.clone())
                            .collect();
                    }
                    Ok(())
                }));
            }
            PlanOperation::RecordEvidence {
                plan_digest,
                result,
            } => {
                index_of(&draft, &result.waypoint_id)?;
                let current = build_review_plan(&accepted, &source, &ledger)?.plan_digest;
                if plan_digest != current {
                    return Err(plan_error(
                        "revision_conflict",
                        "evidence is bound to a different plan digest than the current plan.",
                        None,
                    ));
                }
                new_ledger.entries.push(EvidenceBinding {
                    result_sha256: sha(&serde_json::to_vec(&result)?),
                    plan_digest,
                    revision: actual + 1,
                });
                adjust = Some(Box::new(move |bundle: &mut MissionBundle| {
                    bundle.manifest.operator_results.push(result.clone());
                    Ok(())
                }));
            }
        }
        draft.revision = actual.checked_add(1).ok_or_else(|| {
            plan_error(
                "revision_exhausted",
                "revision cannot be incremented.",
                None,
            )
        })?;
        let order: Vec<String> = draft
            .waypoints
            .iter()
            .map(|waypoint| waypoint.waypoint_id.clone())
            .collect();
        for (index, waypoint) in draft.waypoints.iter_mut().enumerate() {
            waypoint.depends_on = if index == 0 {
                vec![]
            } else {
                vec![order[index - 1].clone()]
            };
        }
        let recording_evidence = matches!(request.operation, PlanOperation::RecordEvidence { .. });
        let record_name = format!("{REQUEST_DIR}/{}.json", sha(request.request_id.as_bytes()));
        let outcome_digest = RefCell::new(None::<String>);
        let extras = |next: &AcceptedMission| -> Result<Vec<(String, Vec<u8>)>, MissionError> {
            let plan = build_review_plan(next, &draft, &new_ledger)?;
            *outcome_digest.borrow_mut() = Some(plan.plan_digest.clone());
            let record = RequestRecord {
                schema_version: 1,
                mission_id,
                request_id: request.request_id.clone(),
                request_sha256: request_sha.clone(),
                revision: next.bundle.manifest.revision,
                plan_digest: plan.plan_digest,
            };
            let mut files = vec![(record_name.clone(), serde_json::to_vec_pretty(&record)?)];
            if recording_evidence {
                files.push((
                    EVIDENCE_FILE.to_owned(),
                    serde_json::to_vec_pretty(&new_ledger)?,
                ));
            }
            Ok(files)
        };
        let write = PlanWrite {
            adjust: adjust.as_deref(),
            extras: Some(&extras),
        };
        self.save_typed_locked(&draft, actual, true, &write)?;
        let plan_digest = outcome_digest.into_inner().ok_or_else(|| {
            plan_error(
                "projection_diverged",
                "publication did not build a plan.",
                None,
            )
        })?;
        Ok(PlanOperationOutcome {
            revision: actual + 1,
            plan_digest,
            replayed: false,
        })
    }
}
