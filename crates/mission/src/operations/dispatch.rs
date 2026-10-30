use super::*;

pub fn execute(workspace_root: &Path, operation: Operation) -> StatusSnapshot {
    match operation {
        Operation::CreateMission {
            title,
            objective,
            waypoints,
        } => direct::create(workspace_root, title, objective, waypoints),
        Operation::CreateDraft { title, objective } => {
            typed_result(MissionLifecycle::open(workspace_root).create_draft(title, objective))
        }
        Operation::SaveDraft {
            mission_id,
            draft,
            expected_current_revision,
        } => typed_result(MissionLifecycle::open(workspace_root).save_draft(
            mission_id,
            draft,
            expected_current_revision,
        )),
        Operation::PublishDraft {
            mission_id,
            draft,
            expected_current_revision,
        } => typed_result(MissionLifecycle::open(workspace_root).publish_draft(
            mission_id,
            draft,
            expected_current_revision,
        )),
        Operation::GetMissionSource { mission_id } => {
            typed_result(MissionLifecycle::open(workspace_root).get_mission_source(mission_id))
        }
        Operation::RenderMission { mission_id } => {
            let lifecycle = MissionLifecycle::open(workspace_root);
            match lifecycle.get_mission_source(mission_id).and_then(|source| {
                lifecycle
                    .render_mission(mission_id)
                    .map(|rendered| (source, rendered))
            }) {
                Ok((source, roadmap)) => StatusSnapshot {
                    status: "ok".into(),
                    mission_id: Some(mission_id),
                    revision: Some(source.revision),
                    mission_state: None,
                    publication_state: None,
                    diagnostics: vec![],
                    rendered: Some(RenderedPreview {
                        roadmap,
                        parts: BTreeMap::new(),
                    }),
                    accepted: None,
                    source: Some(source),
                    migration_report: None,
                    typed_migration: None,
                    repair: None,
                    output_record: None,
                    external_status_snapshot: BTreeMap::new(),
                },
                Err(error) => failure(Some(mission_id), error),
            }
        }
        Operation::EditMissionObjective {
            mission_id,
            objective,
            expected_current_revision,
        } => direct::edit_objective(
            workspace_root,
            mission_id,
            objective,
            expected_current_revision,
        ),
        Operation::AddWaypoint {
            mission_id,
            waypoint,
            expected_current_revision,
        } => direct::add_waypoint(
            workspace_root,
            mission_id,
            waypoint,
            expected_current_revision,
        ),
        Operation::EditWaypoint {
            mission_id,
            waypoint,
            expected_current_revision,
        } => direct::edit_waypoint(
            workspace_root,
            mission_id,
            waypoint,
            expected_current_revision,
        ),
        Operation::OrderWaypoints {
            mission_id,
            execution_order,
            waypoint_dependencies,
            expected_current_revision,
        } => direct::order_waypoints(
            workspace_root,
            mission_id,
            execution_order,
            waypoint_dependencies,
            expected_current_revision,
        ),
        Operation::RecordProgress {
            mission_id,
            waypoint_id,
            state,
            expected_current_revision,
        } => direct::record_progress(
            workspace_root,
            mission_id,
            waypoint_id,
            state,
            expected_current_revision,
        ),
        Operation::RecordValidationResult {
            mission_id,
            result,
            expected_current_revision,
        } => direct::record_result(
            workspace_root,
            mission_id,
            result,
            expected_current_revision,
        ),
        Operation::ValidateMission { mission_id } => direct::validate(workspace_root, mission_id),
        Operation::PublishMission {
            mission_id,
            expected_current_revision,
        } => direct::publish(workspace_root, mission_id, expected_current_revision),
        Operation::Get { mission_id } => {
            let mut snapshot = inspect_mission(workspace_root, mission_id, true);
            if snapshot.status != "error" {
                snapshot.status = "ok".into();
                if snapshot.publication_state == Some(PublicationState::NotPublished) {
                    snapshot.diagnostics.clear();
                }
            }
            snapshot
        }
        Operation::ValidatePreview { manifest_path } => match collect_bundle(&manifest_path) {
            Ok(bundle) => success(None, Some(bundle.manifest.revision)),
            Err(error) => failure(None, error),
        },
        Operation::Import {
            mission_id,
            manifest_path,
            expected_current_revision,
        } => import_bundle(
            workspace_root,
            &manifest_path,
            mission_id,
            expected_current_revision,
        ),
        Operation::RenderPreview { mission_id } => {
            let store = MissionStore::open(workspace_root);
            match store.get(mission_id).and_then(|accepted| {
                let snapshot =
                    ticket_snapshot(workspace_root, &store, mission_id, &accepted.bundle)?;
                let states = snapshot_states(&snapshot)?;
                let snapshot_id = snapshot_id(&snapshot)?;
                let rendered =
                    store.render_with_status_snapshot(mission_id, &states, &snapshot_id)?;
                let mission_state =
                    store.derived_state_with_status_snapshot(mission_id, &states)?;
                let roadmap = String::from_utf8(rendered.roadmap).map_err(|error| {
                    MissionError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
                })?;
                let parts = rendered
                    .parts
                    .into_iter()
                    .map(|(name, bytes)| {
                        String::from_utf8(bytes)
                            .map(|contents| (name, contents))
                            .map_err(|error| {
                                MissionError::Io(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    error,
                                ))
                            })
                    })
                    .collect::<Result<_, _>>()?;
                Ok((
                    accepted,
                    snapshot,
                    mission_state,
                    RenderedPreview { roadmap, parts },
                ))
            }) {
                Ok((accepted, external_status_snapshot, mission_state, rendered)) => {
                    StatusSnapshot {
                        status: "ok".into(),
                        mission_id: Some(mission_id),
                        revision: Some(accepted.bundle.manifest.revision),
                        mission_state: Some(mission_state),
                        publication_state: None,
                        diagnostics: vec![],
                        rendered: Some(rendered),
                        accepted: None,
                        source: None,
                        migration_report: None,
                        typed_migration: None,
                        repair: None,
                        output_record: None,
                        external_status_snapshot,
                    }
                }
                Err(error) => failure(Some(mission_id), error),
            }
        }
        Operation::Publish {
            mission_id,
            manifest_path,
            expected_current_revision,
        } => publish_bundle(
            workspace_root,
            mission_id,
            &manifest_path,
            expected_current_revision,
        ),
        Operation::CheckGenerated { mission_id } => check_generated(workspace_root, mission_id),
        Operation::GetOutputContract { mission_id } => output_result(
            mission_id,
            (|| {
                let store = MissionStore::open(workspace_root);
                Ok(OutputRecord::Contract {
                    contract: store.output_contract(mission_id)?,
                    binding: store.output_contract_binding(mission_id)?,
                })
            })(),
        ),
        Operation::GetApprovalEvent {
            mission_id,
            revision,
        } => output_result(
            mission_id,
            MissionStore::open(workspace_root)
                .approval_event(mission_id, revision)
                .map(OutputRecord::ApprovalEvent),
        ),
        Operation::RecordApprovalEvent { mission_id, json } => output_result(
            mission_id,
            MissionStore::open(workspace_root)
                .record_approval_event_json(mission_id, json.as_bytes())
                .map(OutputRecord::ApprovalEvent),
        ),
        Operation::PublishWithInputs {
            mission_id,
            manifest_path,
            expected_current_revision,
            inputs,
        } => publish_bundle_with_inputs(
            workspace_root,
            mission_id,
            &manifest_path,
            expected_current_revision,
            Some(&inputs),
        ),
        Operation::MigrateDossiers {
            dossier_path,
            dry_run,
        } => match migration::migrate_dossiers(workspace_root, dossier_path.as_deref(), dry_run) {
            Ok(migration_report) => StatusSnapshot {
                status: "ok".into(),
                mission_id: None,
                revision: None,
                mission_state: None,
                publication_state: None,
                diagnostics: vec![],
                rendered: None,
                accepted: None,
                source: None,
                migration_report: Some(migration_report),
                typed_migration: None,
                repair: None,
                output_record: None,
                external_status_snapshot: BTreeMap::new(),
            },
            Err(error) => failure(None, error),
        },
        Operation::MigrateTypedStores { roots } => {
            let roots = if roots.is_empty() {
                vec![workspace_root.to_path_buf()]
            } else {
                roots
            };
            match MissionLifecycle::plan_migration(&roots).and_then(|mut journal| {
                MissionLifecycle::apply_migration(&mut journal)?;
                Ok(journal)
            }) {
                Ok(journal) => StatusSnapshot {
                    status: if journal
                        .records
                        .iter()
                        .all(|record| matches!(record.state, MigrationState::Applied))
                    {
                        "ok"
                    } else {
                        "error"
                    }
                    .into(),
                    mission_id: None,
                    revision: None,
                    mission_state: None,
                    publication_state: None,
                    diagnostics: vec![],
                    rendered: None,
                    accepted: None,
                    source: None,
                    migration_report: None,
                    typed_migration: Some(journal),
                    repair: None,
                    output_record: None,
                    external_status_snapshot: BTreeMap::new(),
                },
                Err(error) => failure(None, error),
            }
        }
        Operation::RepairPreview { request } => repair_result(
            MissionLifecycle::open(workspace_root).repair_preview(workspace_root, request),
        ),
        Operation::RepairApply {
            request,
            proposal_id,
        } => repair_result(MissionLifecycle::open(workspace_root).repair_apply(
            workspace_root,
            request,
            &proposal_id,
        )),
        Operation::RepairJournal { journal_id } => repair_result(
            MissionLifecycle::open(workspace_root)
                .repair_journal(workspace_root, journal_id)
                .map(|journal| response_from_journal(&journal)),
        ),
        Operation::RepairResume { journal_id } => repair_result(
            MissionLifecycle::open(workspace_root).repair_resume(workspace_root, journal_id),
        ),
        Operation::RepairRollback { journal_id } => repair_result(
            MissionLifecycle::open(workspace_root).repair_rollback(workspace_root, journal_id),
        ),
    }
}
