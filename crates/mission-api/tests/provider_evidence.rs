mod provider_evidence {
    pub mod support;
}
use mission_api::*;
use provider_evidence::support::*;
use std::{path::Path, sync::Arc};

fn contract() -> WaypointExecutionContractV4 {
    let source: TypedDraft =
        serde_json::from_str(include_str!("fixtures/execution-contract-v4.json")).unwrap();
    source.waypoints[0].execution_contract.clone().unwrap()
}

fn verify(
    change: impl FnOnce(&mut CompletionEvidenceV4),
    allow_mock: bool,
) -> Result<VerifiedEvidenceV4, LifecycleError> {
    let provider = Arc::new(TestProvider::new());
    let contract = contract();
    let binding = binding(&contract);
    let mut record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Completion,
        "opaque-receipt",
    );
    change(&mut record);
    let reference = provider.put(&record);
    EvidenceAdmissionV4::test_only(provider).completion(&reference, &binding, &contract, allow_mock)
}

#[test]
fn current_test_only_provider_receipt_passes() {
    assert!(verify(|_| {}, false).is_ok());
}

macro_rules! rejects {
    ($name:ident, $code:literal, $change:expr) => {
        #[test]
        fn $name() {
            assert_eq!(verify($change, false).err().unwrap().code(), $code);
        }
    };
}
rejects!(wrong_provider, "provider_identity_mismatch", |r| r
    .binding
    .provider
    .push('x'));
rejects!(wrong_route, "provider_identity_mismatch", |r| r
    .binding
    .route_id
    .push('x'));
rejects!(wrong_receipt_class, "provider_identity_mismatch", |r| r
    .binding
    .receipt_class
    .push('x'));
rejects!(wrong_version, "provider_version_incompatible", |r| r
    .binding
    .provider_version
    .push('x'));
rejects!(wrong_schema_version, "provider_version_incompatible", |r| {
    r.schema_version = 3
});
rejects!(wrong_root, "provider_root_mismatch", |r| r
    .binding
    .owning_root
    .push('x'));
rejects!(wrong_profile, "provider_profile_mismatch", |r| r
    .binding
    .profile
    .push('x'));
rejects!(changed_environment, "provider_profile_mismatch", |r| r
    .binding
    .environment_digest =
    evidence_content_digest(b"changed"));
rejects!(wrong_waypoint, "provider_waypoint_mismatch", |r| r
    .binding
    .waypoint_id
    .push('x'));
rejects!(wrong_mission, "provider_waypoint_mismatch", |r| r
    .binding
    .mission_id =
    uuid::Uuid::nil());
rejects!(wrong_source, "provider_digest_mismatch", |r| r
    .binding
    .source_digest =
    evidence_content_digest(b"other"));
rejects!(wrong_intent, "provider_digest_mismatch", |r| r
    .binding
    .intent_chain_digest =
    evidence_content_digest(b"other"));
rejects!(wrong_plan, "provider_digest_mismatch", |r| r
    .binding
    .plan_digest =
    evidence_content_digest(b"other"));
rejects!(wrong_contract, "provider_digest_mismatch", |r| r
    .binding
    .contract_digest =
    evidence_content_digest(b"other"));
rejects!(changed_prerequisite, "provider_digest_mismatch", |r| {
    r.binding
        .prerequisite_artifact_digests
        .insert("a".into(), evidence_content_digest(b"other"));
});
rejects!(stale_receipt, "provider_evidence_stale", |r| r.validity =
    ProviderValidityV4::Stale);
rejects!(revoked_receipt, "provider_evidence_revoked", |r| r
    .validity =
    ProviderValidityV4::Revoked);
rejects!(unready_provider, "provider_unready", |r| r.ready = false);
rejects!(failed_check, "automated_check_failed", |r| r.checks[0]
    .failed = 1);
rejects!(skipped_check, "automated_check_failed", |r| r.checks[0]
    .skipped =
    1);
rejects!(zero_execution, "zero_executed_checks", |r| r.checks[0]
    .executed =
    0);
rejects!(missing_check, "automated_check_evidence_required", |r| {
    r.checks.pop();
});
rejects!(duplicate_check, "automated_check_evidence_required", |r| r
    .checks
    .push(r.checks[0].clone()));
rejects!(wrong_check_hash, "check_metadata_mismatch", |r| r.checks
    [0]
.check_digest =
    evidence_content_digest(b"forged command"));
rejects!(wrong_class, "evidence_class_mismatch", |r| r.checks[0]
    .evidence_class =
    ExecutionEvidenceClassV4::ResearchRecord);
rejects!(missing_postcondition, "condition_evidence_required", |r| r
    .condition_ids
    .clear());
rejects!(mock_diagnostic_only, "mock_evidence_prohibited", |r| r
    .checks[0]
    .mock =
    true);
rejects!(
    second_diagnostic_rerun_blocks,
    "failure_bound_exhausted",
    |r| {
        r.diagnostic_reruns = 2;
        r.fix_justification = Some("fix".into());
    }
);
rejects!(unjustified_rerun_blocks, "diagnostic_fix_required", |r| r
    .diagnostic_reruns =
    1);

#[test]
fn one_justified_rerun_and_confirmed_mock_policy_can_pass() {
    assert!(
        verify(
            |r| {
                r.diagnostic_reruns = 1;
                r.fix_justification = Some("Fixed the observed failure".into());
                r.checks[0].mock = true;
            },
            true
        )
        .is_ok()
    );
}

#[test]
fn production_cannot_select_fixture_even_when_receipt_exists() {
    let provider = TestProvider::new();
    let contract = contract();
    let binding = binding(&contract);
    let record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Completion,
        "fixture",
    );
    let reference = provider.put(&record);
    assert_eq!(
        EvidenceAdmissionV4::default()
            .completion(&reference, &binding, &contract, true)
            .err()
            .unwrap()
            .code(),
        "capability_unavailable"
    );
}

#[test]
fn missing_forged_and_corrupt_provider_receipts_block() {
    let provider = Arc::new(TestProvider::new());
    let contract = contract();
    let binding = binding(&contract);
    let record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Completion,
        "stored",
    );
    let reference = provider.put(&record);
    let admission = EvidenceAdmissionV4::test_only(provider.clone());
    for receipt in ["", "forged", "/caller/path.json"] {
        let reference = ProviderEvidenceRefV4 {
            receipt: receipt.into(),
        };
        assert_eq!(
            admission
                .completion(&reference, &binding, &contract, false)
                .err()
                .unwrap()
                .code(),
            "provider_receipt_missing"
        );
    }
    let filename = provider.records.lock().unwrap()["stored"].0.clone();
    std::fs::write(provider.dir.path().join(filename), b"corrupted bytes").unwrap();
    assert_eq!(
        admission
            .completion(&reference, &binding, &contract, false)
            .err()
            .unwrap()
            .code(),
        "provider_evidence_corrupt"
    );
}

#[test]
fn preflight_rereads_environment_and_explicit_validity() {
    let provider = Arc::new(TestProvider::new());
    let contract = contract();
    let mut binding = binding(&contract);
    let mut record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Preflight,
        "preflight",
    );
    provider.put(&record);
    let admission = EvidenceAdmissionV4::test_only(provider.clone());
    admission.preflight(&binding, &contract).unwrap();
    *provider.environment.lock().unwrap() = evidence_content_digest(b"changed-profile");
    binding.environment_digest = admission
        .current_environment(&binding.owning_root, &binding.profile)
        .unwrap();
    assert_eq!(
        admission
            .preflight(&binding, &contract)
            .err()
            .unwrap()
            .code(),
        "provider_profile_mismatch"
    );
    record.binding = binding.clone();
    record.validity = ProviderValidityV4::Revoked;
    provider.put(&record);
    assert_eq!(
        admission
            .preflight(&binding, &contract)
            .err()
            .unwrap()
            .code(),
        "provider_evidence_revoked"
    );
}

#[test]
fn returned_receipt_identity_and_purpose_cannot_be_substituted() {
    let provider = Arc::new(TestProvider::new());
    let contract = contract();
    let binding = binding(&contract);
    let mut record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Completion,
        "other-receipt",
    );
    provider.put(&record);
    let entry = provider.records.lock().unwrap()["other-receipt"].clone();
    provider
        .records
        .lock()
        .unwrap()
        .insert("expected-receipt".into(), entry);
    let admission = EvidenceAdmissionV4::test_only(provider.clone());
    let reference = ProviderEvidenceRefV4 {
        receipt: "expected-receipt".into(),
    };
    assert_eq!(
        admission
            .completion(&reference, &binding, &contract, false)
            .err()
            .unwrap()
            .code(),
        "provider_receipt_mismatch"
    );
    record.reference = reference.clone();
    record.purpose = EvidencePurposeV4::Preflight;
    provider.put(&record);
    assert_eq!(
        admission
            .completion(&reference, &binding, &contract, false)
            .err()
            .unwrap()
            .code(),
        "provider_receipt_mismatch"
    );
}

#[test]
fn condition_links_require_checks_even_when_their_kind_is_preflight() {
    let provider = Arc::new(TestProvider::new());
    let mut contract = contract();
    contract.postconditions[0]
        .check_ids
        .push("check-preflight".into());
    let binding = binding(&contract);
    let mut record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Completion,
        "linked-check",
    );
    record.checks.retain(|c| c.check_id != "check-preflight");
    let reference = provider.put(&record);
    assert_eq!(
        EvidenceAdmissionV4::test_only(provider)
            .completion(&reference, &binding, &contract, false)
            .err()
            .unwrap()
            .code(),
        "automated_check_evidence_required"
    );
}

#[test]
fn current_prerequisite_artifact_drift_requires_new_preflight() {
    let provider = Arc::new(TestProvider::new());
    let mut contract = contract();
    contract.declared_dependency_ids = vec!["a".into()];
    contract.no_dependencies_reason = None;
    let mut binding = binding(&contract);
    binding
        .prerequisite_artifact_digests
        .insert("a".into(), evidence_content_digest(b"producer-v1"));
    let record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Preflight,
        "preflight-v1",
    );
    provider.put(&record);
    let admission = EvidenceAdmissionV4::test_only(provider);
    admission.preflight(&binding, &contract).unwrap();
    binding
        .prerequisite_artifact_digests
        .insert("a".into(), evidence_content_digest(b"producer-v2"));
    assert_eq!(
        admission
            .preflight(&binding, &contract)
            .err()
            .unwrap()
            .code(),
        "provider_digest_mismatch"
    );
}

#[test]
fn deterministic_document_schema_link_example_checks_are_accepted() {
    let provider = Arc::new(TestProvider::new());
    let mut contract = contract();
    contract.outcome_category = WaypointOutcomeCategoryV4::Artifact;
    contract.work_kind = WaypointWorkKind::Documentation;
    for check in &mut contract.checks {
        check.evidence_class = ExecutionEvidenceClassV4::DeterministicArtifact;
    }
    for requirement in &mut contract.evidence_requirements {
        requirement.class = ExecutionEvidenceClassV4::DeterministicArtifact;
    }
    let binding = binding(&contract);
    let record = record(
        &contract,
        binding.clone(),
        EvidencePurposeV4::Completion,
        "document",
    );
    let reference = provider.put(&record);
    assert!(
        EvidenceAdmissionV4::test_only(provider)
            .completion(&reference, &binding, &contract, false)
            .is_ok()
    );
}

#[test]
fn exact_handover_content_schema_root_and_consumer_are_required() {
    let endpoint = |waypoint: &str| ExecutionHandoverEndpointV4 {
        waypoint_id: waypoint.into(),
        artifact_id: "artifact".into(),
        content_reference: "out/contract.json".into(),
        schema: "schema/4".into(),
        root: "out".into(),
        check_ids: vec!["check".into()],
    };
    let expected = ExecutionHandoverV4 {
        id: "handover".into(),
        producer: endpoint("a"),
        consumer: endpoint("b"),
    };
    let digest = evidence_content_digest(b"contract");
    let artifact = HandoverArtifactV4 {
        id: "handover".into(),
        producer: expected.producer.clone(),
        consumer: expected.consumer.clone(),
        producer_contract_digest: digest.clone(),
        consumer_contract_digest: digest.clone(),
        owning_root: "/fixture/out".into(),
        content_sha256: evidence_content_digest(b"{}"),
        content: b"{}".to_vec(),
        schema_valid: true,
    };
    let check = |a: &HandoverArtifactV4| {
        validate_handover_artifact(a, &expected, Path::new("/fixture"), &digest, &digest)
    };
    check(&artifact).unwrap();
    let cases: Vec<(&str, HandoverArtifactV4)> = vec![
        (
            "handover_content_mismatch",
            HandoverArtifactV4 {
                content: vec![],
                ..artifact.clone()
            },
        ),
        (
            "handover_content_mismatch",
            HandoverArtifactV4 {
                content: b"tampered".to_vec(),
                ..artifact.clone()
            },
        ),
        (
            "handover_root_mismatch",
            HandoverArtifactV4 {
                owning_root: "/elsewhere".into(),
                ..artifact.clone()
            },
        ),
        (
            "handover_schema_invalid",
            HandoverArtifactV4 {
                schema_valid: false,
                ..artifact.clone()
            },
        ),
        (
            "handover_contract_mismatch",
            HandoverArtifactV4 {
                consumer_contract_digest: evidence_content_digest(b"other"),
                ..artifact.clone()
            },
        ),
        (
            "handover_contract_mismatch",
            HandoverArtifactV4 {
                consumer: endpoint("other"),
                ..artifact.clone()
            },
        ),
        (
            "handover_contract_mismatch",
            HandoverArtifactV4 {
                producer: ExecutionHandoverEndpointV4 {
                    schema: "wrong".into(),
                    ..artifact.producer.clone()
                },
                ..artifact
            },
        ),
    ];
    for (code, artifact) in cases {
        assert_eq!(check(&artifact).unwrap_err().code(), code);
    }
}
