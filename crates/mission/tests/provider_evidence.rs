#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "provider_evidence/handover.rs"]
mod handover;
#[path = "support/host_hook.rs"]
mod host_hook;
#[path = "provider_evidence/mock_policy.rs"]
mod mock_policy;
#[path = "support/provider_fixture.rs"]
mod provider_fixture;

use mission::*;
use std::sync::Arc;
use uuid::Uuid;

fn request(f: &approval_fixture::Fx, reason: &str) -> ControlRequest {
    ControlRequest {
        op_id: Uuid::new_v4(),
        expected_control_sequence: WorkspaceLifecycle::open(&f.ws)
            .read_lifecycle(&WaypointIdentity::Mission { mission_id: f.id })
            .unwrap()
            .control_sequence,
        evidence: Some(reason.into()),
    }
}

fn evidence(f: &approval_fixture::Fx, waypoint: &str, pointer: &str) {
    let plan = f.plan();
    MissionLifecycle::open(&f.ws)
        .apply_plan_operation(
            f.id,
            PlanOperationRequest {
                request_id: format!("evidence-{waypoint}"),
                expected_revision: plan.revision,
                operation: PlanOperation::RecordEvidence {
                    plan_digest: plan.plan_digest,
                    result: OperatorResult {
                        waypoint_id: waypoint.into(),
                        outcome: "passed".into(),
                        operator_command: "cargo test".into(),
                        evidence_pointer: pointer.into(),
                    },
                },
            },
        )
        .unwrap();
}

fn runtime_contract(f: &approval_fixture::Fx) {
    let lifecycle = MissionLifecycle::open(&f.ws);
    let mut source = lifecycle.get_mission_source(f.id).unwrap();
    let contract = source.waypoints[0].execution_contract.as_mut().unwrap();
    contract.provider_route = Some(ExecutionProviderRouteV4 {
        id: "runtime".into(),
        provider: "sandbox-future".into(),
        version: "4".into(),
        receipt_class: "runtime-evidence".into(),
        preflight_check_ids: contract.environment.preflight_check_ids.clone(),
    });
    contract.no_provider_reason = None;
    lifecycle
        .publish_draft(f.id, source, f.plan().revision)
        .unwrap();
}

#[test]
fn caller_path_and_asserted_pass_cannot_complete_without_provider() {
    let f = approval_fixture::linked();
    f.approve("w7-no-provider", "w7-event");
    ticket_api::storage::store::TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("fixture"))
        .unwrap();
    let plan = f.plan();
    std::fs::create_dir_all(f.ws.join("asserted")).unwrap();
    std::fs::write(f.ws.join("asserted/pass.json"), br#"{"passed":true}"#).unwrap();
    MissionLifecycle::open(&f.ws)
        .apply_plan_operation(
            f.id,
            PlanOperationRequest {
                request_id: "caller-assertion".into(),
                expected_revision: plan.revision,
                operation: PlanOperation::RecordEvidence {
                    plan_digest: plan.plan_digest,
                    result: OperatorResult {
                        waypoint_id: "a".into(),
                        outcome: "passed".into(),
                        operator_command: "cargo test".into(),
                        evidence_pointer: "asserted/pass.json".into(),
                    },
                },
            },
        )
        .unwrap();
    let result = WorkspaceLifecycle::open(&f.ws).complete_waypoint(
        f.id,
        "a",
        &ControlRequest {
            op_id: Uuid::new_v4(),
            expected_control_sequence: 0,
            evidence: Some("asserted/pass.json".into()),
        },
    );
    assert_eq!(result.unwrap_err().code(), "capability_unavailable");
}

#[test]
fn test_provider_completes_and_production_does_not_promote_its_store() {
    let f = approval_fixture::linked();
    f.approve("provider-success", "provider-event");
    ticket_api::storage::store::TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("fixture"))
        .unwrap();
    evidence(&f, "a", "ce://default/test/completion-a");
    let provider = Arc::new(provider_fixture::TestProvider::new());
    provider_fixture::put(&f, "a", "ce://default/test/completion-a", &provider);
    assert_eq!(
        WorkspaceLifecycle::open(&f.ws)
            .complete_waypoint(f.id, "a", &request(&f, "ce://default/test/completion-a"))
            .unwrap_err()
            .code(),
        "capability_unavailable"
    );
    let adapter = WorkspaceLifecycle::open(&f.ws).with_test_evidence_provider(provider);
    adapter
        .complete_waypoint(f.id, "a", &request(&f, "ce://default/test/completion-a"))
        .unwrap();
    adapter.activate(f.id, &request(&f, "go")).unwrap();
    assert_eq!(
        WorkspaceLifecycle::open(&f.ws)
            .complete_waypoint(f.id, "b", &request(&f, "ce://default/test/completion-b"))
            .unwrap_err()
            .code(),
        "completion_evidence_required"
    );
    f.edit_b("provider-semantic-edit", "changed boundary");
    assert_eq!(f.code(approval::EntryPath::Dispatch), "approval_stale");
}

#[test]
#[cfg(feature = "cli")]
fn production_runtime_activation_resume_dispatch_direct_named_generic_and_cli_fail_closed() {
    let f = approval_fixture::linked();
    runtime_contract(&f);
    f.approve("runtime", "runtime-event");
    for path in [
        approval::EntryPath::Activation,
        approval::EntryPath::Resume,
        approval::EntryPath::Dispatch,
    ] {
        assert_eq!(
            f.approval().check_entry(f.id, path).unwrap_err().code(),
            "capability_unavailable"
        );
    }
    assert_eq!(
        WorkspaceLifecycle::open(&f.ws)
            .activate(f.id, &request(&f, "go"))
            .unwrap_err()
            .code(),
        "capability_unavailable"
    );
    assert_eq!(
        execute(
            &f.ws,
            Operation::RecordProgress {
                mission_id: f.id,
                waypoint_id: "a".into(),
                state: WaypointState::InProgress,
                expected_current_revision: f.plan().revision,
            }
        )
        .diagnostics[0]
            .code,
        "capability_unavailable"
    );
    assert_eq!(
        WorkspaceLifecycle::open(&f.ws)
            .transition(
                &WaypointIdentity::Ticket {
                    ticket_id: f.ticket
                },
                &TransitionRequest {
                    op_id: Uuid::new_v4(),
                    to_native_state: "in-implementation".into(),
                    author: Some("caller".into())
                }
            )
            .unwrap_err()
            .code(),
        "capability_unavailable"
    );
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mission"))
        .args([
            "--workspace",
            f.ws.to_str().unwrap(),
            "record-progress",
            &f.id.to_string(),
            "a",
            "in-progress",
            &f.plan().revision.to_string(),
        ])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(
        text.contains("capability_unavailable"),
        "{text}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    #[cfg(feature = "mcp")]
    {
        let out = mission::named::call(
            "mission_waypoint_activate",
            serde_json::json!({
                "workspace_root": f.ws, "mission_id": f.id, "waypoint_id": "a",
                "request_id": Uuid::new_v4(), "expected_revision": f.plan().revision,
                "author": "requester",
            }),
        );
        assert_eq!(out.code, "capability_unavailable", "{out:?}");
        assert!(out.result.is_none());
        assert_eq!(
            out.details,
            Some(serde_json::json!({"provider_code": "capability_unavailable"}))
        );
        assert_eq!(out.capability, mission::named::CAPABILITY);
        let out = mission::named::call(
            "mission_resume",
            serde_json::json!({
                "workspace_root": f.ws, "mission_id": f.id,
                "request_id": Uuid::new_v4(), "expected_revision": f.plan().revision,
                "expected_control_sequence": 0, "evidence": "resume with no provider",
            }),
        );
        assert_eq!(out.code, "capability_unavailable", "{out:?}");
    }
}

#[test]
fn preflight_current_reread_profile_drift_revocation_and_replay_divergence() {
    let f = approval_fixture::linked();
    runtime_contract(&f);
    f.approve("drift", "drift-event");
    let provider = Arc::new(provider_fixture::TestProvider::new());
    let plan = f.plan();
    let contract = plan.waypoints[0].execution_contract.as_ref().unwrap();
    let binding = provider_fixture::binding(&f, "a", &provider);
    let mut record = provider_fixture::provider::record(
        contract,
        binding.clone(),
        EvidencePurposeV4::Preflight,
        "preflight",
    );
    provider.put(&record);
    let approval = || f.approval().with_test_evidence_provider(provider.clone());
    approval()
        .check_entry(f.id, approval::EntryPath::Dispatch)
        .unwrap();
    *provider.environment.lock().unwrap() = evidence_content_digest(b"changed");
    let error = approval()
        .check_entry(f.id, approval::EntryPath::Resume)
        .unwrap_err();
    assert_eq!(error.code(), "provider_profile_mismatch", "{error}");
    *provider.environment.lock().unwrap() = binding.environment_digest;
    record.validity = ProviderValidityV4::Revoked;
    provider.put(&record);
    assert_eq!(
        approval()
            .check_entry(f.id, approval::EntryPath::Dispatch)
            .unwrap_err()
            .code(),
        "provider_evidence_revoked"
    );
    record.validity = ProviderValidityV4::Current;
    record.fix_justification = Some("different bytes under the same opaque receipt".into());
    provider.put(&record);
    assert_eq!(
        approval()
            .check_entry(f.id, approval::EntryPath::Dispatch)
            .unwrap_err()
            .code(),
        "provider_replay_divergence"
    );
}

#[test]
fn failed_zero_skipped_and_mock_checks_cannot_complete_and_failure_bound_is_finite() {
    let f = approval_fixture::linked();
    f.approve("checks", "checks-event");
    ticket_api::storage::store::TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("fixture"))
        .unwrap();
    let receipt = "ce://default/test/checks-a";
    evidence(&f, "a", receipt);
    let provider = Arc::new(provider_fixture::TestProvider::new());
    let original = provider_fixture::put(&f, "a", receipt, &provider);
    let adapter = WorkspaceLifecycle::open(&f.ws).with_test_evidence_provider(provider.clone());
    for (code, mode) in [
        ("automated_check_failed", 0),
        ("zero_executed_checks", 1),
        ("automated_check_failed", 2),
        ("mock_evidence_prohibited", 3),
        ("failure_bound_exhausted", 4),
    ] {
        let mut record = original.clone();
        match mode {
            0 => record.checks[0].failed = 1,
            1 => record.checks[0].executed = 0,
            2 => record.checks[0].skipped = 1,
            3 => record.checks[0].mock = true,
            _ => {
                record.diagnostic_reruns = 2;
                record.fix_justification = Some("fixed".into());
            }
        }
        provider.put(&record);
        assert_eq!(
            adapter
                .complete_waypoint(f.id, "a", &request(&f, receipt))
                .unwrap_err()
                .code(),
            code
        );
    }
    provider.put(&original);
    adapter
        .complete_waypoint(f.id, "a", &request(&f, receipt))
        .unwrap();
    assert_eq!(
        provider_fixture::adapter(&f)
            .read_lifecycle(&WaypointIdentity::Mission { mission_id: f.id })
            .unwrap()
            .control_sequence,
        1
    );
}

#[test]
fn completion_metadata_is_checked_at_the_actual_lifecycle_guard() {
    let f = approval_fixture::linked();
    f.approve("metadata", "metadata-event");
    ticket_api::storage::store::TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("fixture"))
        .unwrap();
    let receipt = "ce://default/test/metadata-a";
    evidence(&f, "a", receipt);
    let provider = Arc::new(provider_fixture::TestProvider::new());
    let original = provider_fixture::put(&f, "a", receipt, &provider);
    let adapter = WorkspaceLifecycle::open(&f.ws).with_test_evidence_provider(provider.clone());
    for (code, mode) in [
        ("provider_root_mismatch", 0),
        ("provider_profile_mismatch", 1),
        ("provider_waypoint_mismatch", 2),
        ("provider_identity_mismatch", 3),
        ("provider_version_incompatible", 4),
        ("provider_digest_mismatch", 5),
        ("provider_unready", 6),
        ("provider_evidence_stale", 7),
        ("provider_evidence_revoked", 8),
        ("check_metadata_mismatch", 9),
        ("evidence_class_mismatch", 10),
    ] {
        let mut record = original.clone();
        match mode {
            0 => record.binding.owning_root.push('x'),
            1 => record.binding.profile.push('x'),
            2 => record.binding.waypoint_id.push('x'),
            3 => record.binding.provider.push('x'),
            4 => record.binding.provider_version.push('x'),
            5 => record.binding.contract_digest = evidence_content_digest(b"wrong-contract"),
            6 => record.ready = false,
            7 => record.validity = ProviderValidityV4::Stale,
            8 => record.validity = ProviderValidityV4::Revoked,
            9 => record.checks[0].check_digest = evidence_content_digest(b"wrong-command"),
            _ => record.checks[0].evidence_class = ExecutionEvidenceClassV4::CapturedDecision,
        }
        provider.put(&record);
        assert_eq!(
            adapter
                .complete_waypoint(f.id, "a", &request(&f, receipt))
                .unwrap_err()
                .code(),
            code
        );
    }
    let mut entry = provider.records.lock().unwrap()[receipt].clone();
    entry.1 = evidence_content_digest(b"wrong-provider-hash");
    provider
        .records
        .lock()
        .unwrap()
        .insert(receipt.into(), entry);
    assert_eq!(
        adapter
            .complete_waypoint(f.id, "a", &request(&f, receipt))
            .unwrap_err()
            .code(),
        "provider_evidence_corrupt"
    );
    provider.records.lock().unwrap().remove(receipt);
    assert_eq!(
        adapter
            .complete_waypoint(f.id, "a", &request(&f, receipt))
            .unwrap_err()
            .code(),
        "provider_receipt_missing"
    );
}

#[test]
fn document_outcome_completes_with_trusted_deterministic_artifact_checks() {
    let f = approval_fixture::linked();
    let lifecycle = MissionLifecycle::open(&f.ws);
    let mut source = lifecycle.get_mission_source(f.id).unwrap();
    let waypoint = &mut source.waypoints[0];
    waypoint.work_kind = Some(WaypointWorkKind::Documentation);
    let contract = waypoint.execution_contract.as_mut().unwrap();
    contract.work_kind = WaypointWorkKind::Documentation;
    contract.outcome_category = WaypointOutcomeCategoryV4::Artifact;
    for check in &mut contract.checks {
        check.command = format!("document-schema-link-example-check {}", check.id);
        check.evidence_class = ExecutionEvidenceClassV4::DeterministicArtifact;
    }
    for requirement in &mut contract.evidence_requirements {
        requirement.class = ExecutionEvidenceClassV4::DeterministicArtifact;
    }
    lifecycle
        .publish_draft(f.id, source, f.plan().revision)
        .unwrap();
    f.approve("document", "document-event");
    ticket_api::storage::store::TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("fixture"))
        .unwrap();
    let receipt = "ce://default/test/document-a";
    evidence(&f, "a", receipt);
    provider_fixture::adapter(&f)
        .complete_waypoint(f.id, "a", &request(&f, receipt))
        .unwrap();
}
