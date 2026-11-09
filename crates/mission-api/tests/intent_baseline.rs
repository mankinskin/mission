use mission_api::*;
use std::fs;

#[path = "intent_baseline/fixture.rs"]
mod fixture;
use fixture::{Provider, fixture, provider};

#[test]
fn confirmed_baseline_survives_objective_changes_and_identical_replay() {
    let (root, store, id, requirements) = fixture();
    let digest = intent_baseline_digest(id, &requirements).unwrap();
    let (reference, provider) = provider(b"Working search", digest);
    let baseline = store
        .confirm_intent_baseline(id, 1, requirements.clone(), reference.clone(), &provider)
        .unwrap();
    let lifecycle = MissionLifecycle::open(root.path());
    let mut draft = lifecycle.get_mission_source(id).unwrap();
    draft.objective = "Design only".into();
    lifecycle.save_draft(id, draft, 1).unwrap();
    assert_eq!(
        store
            .effective_intent(id, &provider)
            .unwrap()
            .unwrap()
            .baseline,
        baseline
    );
    assert_eq!(
        store
            .confirm_intent_baseline(id, 2, requirements, reference, &provider)
            .unwrap(),
        baseline
    );
}

#[test]
fn unreceipted_or_invalid_source_baseline_cannot_be_persisted() {
    let (_root, store, id, mut requirements) = fixture();
    let (reference, _) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    assert!(
        store
            .confirm_intent_baseline(
                id,
                1,
                requirements.clone(),
                reference.clone(),
                &Provider(vec![])
            )
            .is_err()
    );
    requirements[0].source.excerpt = "Design".into();
    assert!(
        store
            .confirm_intent_baseline(id, 1, requirements, reference, &Provider(vec![]))
            .is_err()
    );
}

#[test]
fn legacy_without_intent_remains_readable_without_writes() {
    let (root, store, id, _) = fixture();
    let directory = root
        .path()
        .join(".workflow-tools/mission/missions")
        .join(id.to_string());
    let before = fs::read(directory.join("source.json")).unwrap();
    assert_eq!(store.effective_intent(id, &Provider(vec![])).unwrap(), None);
    assert_eq!(fs::read(directory.join("source.json")).unwrap(), before);
    assert!(!directory.join("intent").exists());
}

#[test]
fn amendments_require_fresh_receipts_preserve_original_and_replay_identically() {
    let (_root, store, id, requirements) = fixture();
    let (reference, mut trusted) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    let baseline = store
        .confirm_intent_baseline(id, 1, requirements, reference, &trusted)
        .unwrap();
    let before = store.effective_intent(id, &trusted).unwrap().unwrap();
    let changes = vec![IntentChangeV4::Exclude {
        prior_requirement_id: "search".into(),
    }];
    let sha = intent_amendment_digest(
        id,
        "exclude-search",
        &before.chain_sha256,
        "Exclude search",
        &changes,
    )
    .unwrap();
    let (reference, next_provider) = provider(b"Exclude search", sha);
    let request = IntentAmendmentRequest {
        expected_revision: 1,
        amendment_id: "exclude-search".into(),
        prior_chain_sha256: before.chain_sha256.clone(),
        source: "Exclude search".into(),
        changes,
        reference,
    };
    assert!(
        store
            .append_intent_amendment(id, request.clone(), &trusted)
            .is_err()
    );
    trusted.0.extend(next_provider.0);
    let amendment = store
        .append_intent_amendment(id, request.clone(), &trusted)
        .unwrap();
    assert_eq!(
        store
            .append_intent_amendment(id, request, &trusted)
            .unwrap(),
        amendment
    );
    let after = store.effective_intent(id, &trusted).unwrap().unwrap();
    assert_eq!(after.baseline, baseline);
    assert!(after.requirements.is_empty());
    assert_eq!(after.excluded_requirement_ids, ["search"]);
}

#[test]
fn conflicting_baseline_invalid_utf8_spans_and_provider_tampering_fail() {
    let (_root, store, id, mut requirements) = fixture();
    let (reference, trusted) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    store
        .confirm_intent_baseline(id, 1, requirements.clone(), reference.clone(), &trusted)
        .unwrap();
    requirements[0].outcome = "Only design".into();
    assert!(
        store
            .confirm_intent_baseline(id, 1, requirements.clone(), reference.clone(), &trusted)
            .is_err()
    );
    requirements[0].source.end = usize::MAX;
    assert!(
        store
            .confirm_intent_baseline(id, 1, requirements, reference, &trusted)
            .is_err()
    );
    let mut tampered = trusted;
    tampered.0[0].requester_event_id = "different-host-event".into();
    assert!(store.effective_intent(id, &tampered).is_err());
}

#[test]
fn corrupt_intent_and_noncontiguous_amendments_are_not_legacy_absence() {
    let (root, store, id, requirements) = fixture();
    let (reference, trusted) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    store
        .confirm_intent_baseline(id, 1, requirements, reference, &trusted)
        .unwrap();
    let directory = root
        .path()
        .join(".workflow-tools/mission/missions")
        .join(id.to_string());
    fs::create_dir_all(directory.join("intent/amendments")).unwrap();
    fs::write(directory.join("intent/amendments/00000002.json"), b"{}").unwrap();
    assert!(store.effective_intent(id, &trusted).is_err());
    fs::remove_dir_all(directory.join("intent/amendments")).unwrap();
    fs::write(directory.join("intent/baseline.json"), b"{").unwrap();
    assert!(store.effective_intent(id, &trusted).is_err());
}

#[test]
fn legacy_sources_history_and_intent_survive_publish_goal_and_requirement_edits() {
    for schema in [2, 3] {
        let (root, store, id, requirements) = fixture();
        let lifecycle = MissionLifecycle::open(root.path());
        let (reference, trusted) = provider(
            b"Working search",
            intent_baseline_digest(id, &requirements).unwrap(),
        );
        let baseline = store
            .confirm_intent_baseline(id, 1, requirements, reference, &trusted)
            .unwrap();
        let mut draft = lifecycle.get_mission_source(id).unwrap();
        draft.schema_version = schema;
        draft.waypoints.push(TypedWaypoint {
            waypoint_id: "search".into(),
            title: "Search".into(),
            prompt: "Implement search".into(),
            part: PartSource {
                waypoint_id: "search".into(),
                document: Document {
                    blocks: vec![Block::Paragraph {
                        inlines: vec![Inline::Text {
                            text: "Implement search".into(),
                        }],
                    }],
                },
            },
            validation: vec![ValidationGate {
                command: "true".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "search checkpoint".into(),
            },
            outcome_plan: (schema == 3).then(|| WaypointOutcomePlan {
                expected_outcome: "Search returns matching records".into(),
                alternative_outcomes: vec![AlternativeOutcome {
                    outcome: "Search cannot be verified".into(),
                    acceptable_handling: vec!["Stop and report the failed gate".into()],
                }],
            }),
            ..Default::default()
        });
        let published = lifecycle.publish_draft(id, draft, 1).unwrap();
        let directory = root
            .path()
            .join(".workflow-tools/mission/missions")
            .join(id.to_string());
        let history_path = directory.join(format!("history/revision-{}.json", published.revision));
        let history = fs::read(&history_path).unwrap();
        let intent_bytes = fs::read(directory.join("intent/baseline.json")).unwrap();
        for (request, operation) in [
            (
                "narrow-goal",
                PlanOperation::SetGoal {
                    title: "Design".into(),
                    objective: "Document only".into(),
                },
            ),
            (
                "narrow-requirements",
                PlanOperation::SetRequirements {
                    requirements: vec![],
                },
            ),
        ] {
            lifecycle
                .apply_plan_operation(
                    id,
                    PlanOperationRequest {
                        request_id: request.into(),
                        expected_revision: lifecycle.get_mission_source(id).unwrap().revision,
                        operation,
                    },
                )
                .unwrap();
            assert_eq!(
                store
                    .effective_intent(id, &trusted)
                    .unwrap()
                    .unwrap()
                    .baseline,
                baseline
            );
        }
        assert_eq!(
            lifecycle.get_mission_source(id).unwrap().schema_version,
            schema
        );
        assert_eq!(fs::read(history_path).unwrap(), history);
        assert_eq!(
            fs::read(directory.join("intent/baseline.json")).unwrap(),
            intent_bytes
        );
    }
}

#[test]
fn stale_revisions_and_amendment_chains_cannot_authorize_changes() {
    let (_root, store, id, requirements) = fixture();
    let (reference, trusted) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    assert!(
        store
            .confirm_intent_baseline(id, 0, requirements.clone(), reference.clone(), &trusted)
            .is_err()
    );
    store
        .confirm_intent_baseline(id, 1, requirements, reference.clone(), &trusted)
        .unwrap();
    let intent = store.effective_intent(id, &trusted).unwrap().unwrap();
    let mut request = IntentAmendmentRequest {
        expected_revision: 0,
        amendment_id: "exclude".into(),
        prior_chain_sha256: intent.chain_sha256,
        source: "Exclude search".into(),
        changes: vec![IntentChangeV4::Exclude {
            prior_requirement_id: "search".into(),
        }],
        reference,
    };
    assert!(
        store
            .proposed_intent_amendment(id, &request, &trusted)
            .is_err()
    );
    request.expected_revision = 1;
    request.prior_chain_sha256 = "0".repeat(64);
    assert!(
        store
            .proposed_intent_amendment(id, &request, &trusted)
            .is_err()
    );
    request.prior_chain_sha256 = store
        .effective_intent(id, &trusted)
        .unwrap()
        .unwrap()
        .chain_sha256;
    request.changes.push(request.changes[0].clone());
    assert!(
        store
            .proposed_intent_amendment(id, &request, &trusted)
            .is_err()
    );
}

#[test]
fn source_must_preserve_utf8_boundaries_and_compatible_evidence() {
    let (root, store, id, mut requirements) = fixture();
    let source = "Build café search";
    fs::write(
        root.path()
            .join(".workflow-tools/mission/missions")
            .join(id.to_string())
            .join("input/submitted-input.bin"),
        source,
    )
    .unwrap();
    requirements[0].source = IntentSourceSpan {
        source_sha256: intent_source_digest(source.as_bytes()),
        start: 0,
        end: source.len(),
        excerpt: source.into(),
    };
    assert!(store.proposed_intent_baseline(id, 1, &requirements).is_ok());
    requirements[0].source.end = 10;
    assert!(
        store
            .proposed_intent_baseline(id, 1, &requirements)
            .is_err()
    );
    requirements[0].source.end = source.len();
    requirements[0].evidence_policy = IntentEvidencePolicy::DeterministicArtifact;
    assert!(
        store
            .proposed_intent_baseline(id, 1, &requirements)
            .is_err()
    );
}

#[test]
fn amendment_reused_authority_ids_and_divergent_replays_are_rejected() {
    let (_root, store, id, requirements) = fixture();
    let (reference, mut trusted) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    let baseline = store
        .confirm_intent_baseline(id, 1, requirements, reference, &trusted)
        .unwrap();
    let intent = store.effective_intent(id, &trusted).unwrap().unwrap();
    let changes = vec![IntentChangeV4::Exclude {
        prior_requirement_id: "search".into(),
    }];
    let sha = intent_amendment_digest(
        id,
        "exclude",
        &intent.chain_sha256,
        "Exclude search",
        &changes,
    )
    .unwrap();
    let (reference, fresh) = provider(b"Exclude search", sha);
    let request = IntentAmendmentRequest {
        expected_revision: 1,
        amendment_id: "exclude".into(),
        prior_chain_sha256: intent.chain_sha256,
        source: "Exclude search".into(),
        changes,
        reference,
    };
    let mut reused = fresh.0[0].clone();
    reused.receipt_id = baseline.receipt.receipt_id.clone();
    trusted.0.push(reused);
    assert!(
        store
            .append_intent_amendment(id, request.clone(), &trusted)
            .is_err()
    );
    trusted.0.pop();
    let mut reused = fresh.0[0].clone();
    reused.requester_event_id = baseline.receipt.requester_event_id;
    trusted.0.push(reused);
    assert!(
        store
            .append_intent_amendment(id, request.clone(), &trusted)
            .is_err()
    );
    trusted.0.pop();
    trusted.0.extend(fresh.0);
    store
        .append_intent_amendment(id, request.clone(), &trusted)
        .unwrap();
    let mut divergent = request;
    divergent.source = "Different amendment source".into();
    assert!(
        store
            .append_intent_amendment(id, divergent, &trusted)
            .is_err()
    );
}

#[test]
fn interrupted_staging_does_not_corrupt_committed_records_and_baseline_retry_recovers() {
    let (root, store, id, requirements) = fixture();
    let store_root = root.path().join(".workflow-tools/mission/missions");
    fs::write(store_root.join(".abandoned.intent-write.tmp"), b"partial").unwrap();
    let directory = store_root.join(id.to_string());
    fs::create_dir(directory.join("intent")).unwrap();
    let (reference, trusted) = provider(
        b"Working search",
        intent_baseline_digest(id, &requirements).unwrap(),
    );
    let error = store.effective_intent(id, &trusted).unwrap_err();
    assert_eq!(error.diagnostics().unwrap()[0].code, "intent-write-pending");
    store
        .confirm_intent_baseline(id, 1, requirements, reference, &trusted)
        .unwrap();
    assert!(store.effective_intent(id, &trusted).unwrap().is_some());
    assert_eq!(fs::read_dir(directory.join("intent")).unwrap().count(), 1);
}
