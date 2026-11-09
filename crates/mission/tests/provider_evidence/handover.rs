use super::*;

fn declared_handover(f: &approval_fixture::Fx) -> ExecutionHandoverV4 {
    let lifecycle = MissionLifecycle::open(&f.ws);
    let mut source = lifecycle.get_mission_source(f.id).unwrap();
    let endpoint = |waypoint: &str, check: &str| ExecutionHandoverEndpointV4 {
        waypoint_id: waypoint.into(),
        artifact_id: "result-schema".into(),
        content_reference: "workflow-tools/mission/result.json".into(),
        schema: "result/4".into(),
        root: "workflow-tools/mission".into(),
        check_ids: vec![check.into()],
    };
    let handover = ExecutionHandoverV4 {
        id: "producer-to-consumer".into(),
        producer: endpoint("a", "check-acceptance"),
        consumer: endpoint("b", "check-preflight"),
    };
    let contract = source.waypoints[0].execution_contract.as_mut().unwrap();
    contract.handover = Some(handover.clone());
    contract.no_handover_reason = None;
    lifecycle
        .publish_draft(f.id, source, f.plan().revision)
        .unwrap();
    handover
}

#[test]
fn missing_tampered_schema_wrong_consumer_handover_block_before_consumer_advance() {
    let f = approval_fixture::linked();
    let handover = declared_handover(&f);
    f.approve("handover", "handover-event");
    ticket_api::storage::store::TicketStore::open(&f.ws)
        .unwrap()
        .close(&f.ticket, "done", Some("fixture"))
        .unwrap();
    let receipt = "ce://default/test/handover-a";
    evidence(&f, "a", receipt);
    let provider = Arc::new(provider_fixture::TestProvider::new());
    let original = provider_fixture::put(&f, "a", receipt, &provider);
    let adapter = WorkspaceLifecycle::open(&f.ws).with_test_evidence_provider(provider.clone());
    assert_eq!(
        adapter
            .complete_waypoint(f.id, "a", &request(&f, receipt))
            .unwrap_err()
            .code(),
        "handover_evidence_required"
    );
    let plan = f.plan();
    let artifact = HandoverArtifactV4 {
        id: handover.id.clone(),
        producer: handover.producer.clone(),
        consumer: handover.consumer.clone(),
        producer_contract_digest: plan.waypoints[0].execution_contract_digest.clone().unwrap(),
        consumer_contract_digest: plan.waypoints[1].execution_contract_digest.clone().unwrap(),
        owning_root: f
            .ws
            .join("workflow-tools/mission")
            .to_string_lossy()
            .into_owned(),
        content: br#"{"schema":4}"#.to_vec(),
        content_sha256: evidence_content_digest(br#"{"schema":4}"#),
        schema_valid: true,
    };
    for (code, mode) in [
        ("handover_content_mismatch", 0),
        ("handover_schema_invalid", 1),
        ("handover_contract_mismatch", 2),
        ("handover_contract_mismatch", 3),
        ("handover_root_mismatch", 4),
    ] {
        let mut bad = artifact.clone();
        match mode {
            0 => bad.content.push(b'x'),
            1 => bad.schema_valid = false,
            2 => bad.consumer.waypoint_id = "other".into(),
            3 => bad.consumer.check_ids = vec!["check-acceptance".into()],
            _ => bad.owning_root.push('x'),
        }
        let mut record = original.clone();
        record.handover = Some(bad);
        provider.put(&record);
        assert_eq!(
            adapter
                .complete_waypoint(f.id, "a", &request(&f, receipt))
                .unwrap_err()
                .code(),
            code
        );
    }
    let mut valid = original;
    valid.handover = Some(artifact);
    provider.put(&valid);
    adapter
        .complete_waypoint(f.id, "a", &request(&f, receipt))
        .unwrap();
    let approval = || f.approval().with_test_evidence_provider(provider.clone());
    let revision = f.plan().revision;
    approval()
        .guard_entry(f.id, approval::EntryPath::Dispatch)
        .unwrap()
        .record_progress("b", WaypointState::InProgress, revision)
        .unwrap();
    assert_eq!(f.plan().plan_digest, plan.plan_digest);
    valid.handover.as_mut().unwrap().content.push(b'x');
    provider.put(&valid);
    let error = approval()
        .guard_entry(f.id, approval::EntryPath::Dispatch)
        .err()
        .unwrap();
    assert_eq!(error.code(), "handover_content_mismatch", "{error}");
    valid.handover.as_mut().unwrap().content.pop();
    valid.handover.as_mut().unwrap().content.push(b' ');
    valid.handover.as_mut().unwrap().content_sha256 =
        evidence_content_digest(&valid.handover.as_ref().unwrap().content);
    provider.put(&valid);
    assert_eq!(
        approval()
            .check_entry(f.id, approval::EntryPath::Dispatch)
            .unwrap_err()
            .code(),
        "provider_replay_divergence"
    );
}

#[test]
fn prerequisite_digest_drift_invalidates_preflight_even_when_other_metadata_matches() {
    let f = approval_fixture::linked();
    runtime_contract(&f);
    f.approve("prereq-drift", "prereq-drift-event");
    let provider = Arc::new(provider_fixture::TestProvider::new());
    let plan = f.plan();
    let contract = plan.waypoints[0].execution_contract.as_ref().unwrap();
    let binding = provider_fixture::binding(&f, "a", &provider);
    let mut record = provider_fixture::provider::record(
        contract,
        binding,
        EvidencePurposeV4::Preflight,
        "preflight",
    );
    record.binding.prerequisite_artifact_digests.insert(
        "changed-dependency".into(),
        evidence_content_digest(b"changed artifact"),
    );
    provider.put(&record);
    let error = f
        .approval()
        .with_test_evidence_provider(provider)
        .check_entry(f.id, approval::EntryPath::Dispatch)
        .unwrap_err();
    assert_eq!(error.code(), "provider_digest_mismatch");
}
