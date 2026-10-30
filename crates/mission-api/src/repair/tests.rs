use super::*;
use serde_json::json;
use tempfile::TempDir;

#[test]
fn repair_apply_stops_after_partial_failure_and_resume_finishes_remaining_records() {
    let workspace = TempDir::new().expect("workspace");
    let lifecycle = MissionLifecycle::open(workspace.path());
    let mission_ids = [
        divergent_mission(&lifecycle, "First repair record"),
        divergent_mission(&lifecycle, "Second repair record"),
        divergent_mission(&lifecycle, "Third repair record"),
    ];
    let (request, proposal_id) = approved_request(&lifecycle, workspace.path(), &mission_ids);
    let journal_id = Uuid::from_u128(0x8c2a_2504_6555_4ca3_9f22_1c8d_4f5a_7b90);
    let blocked_snapshot = lifecycle.repair_snapshot_path(journal_id, mission_ids[1]);
    fs::create_dir_all(&blocked_snapshot).expect("create conflicting snapshot");
    fs::write(blocked_snapshot.join("unowned"), b"not a recovery snapshot")
        .expect("write conflicting snapshot");

    let partial = lifecycle
        .repair_apply_with_journal_id(workspace.path(), request, &proposal_id, journal_id)
        .expect("partial repair response");
    let journal = partial.journal.as_ref().expect("durable partial journal");
    assert_eq!(partial.status, "partial");
    assert_eq!(journal.state, RepairJournalState::Partial);
    assert_eq!(journal.records[0].state, RepairRecordState::Applied);
    assert_eq!(journal.records[1].state, RepairRecordState::Failed);
    assert_eq!(journal.records[2].state, RepairRecordState::Pending);
    assert!(
        journal.records[1]
            .diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("repair-snapshot-conflict"))
    );
    assert_eq!(
        lifecycle
            .get(mission_ids[0])
            .expect("first record")
            .bundle
            .manifest
            .revision,
        4
    );
    for mission_id in &mission_ids[1..] {
        assert_eq!(
            lifecycle
                .get(*mission_id)
                .expect("unchanged record")
                .bundle
                .manifest
                .revision,
            3
        );
    }

    let external_edit = lifecycle
        .store
        .root
        .join(mission_ids[0].to_string())
        .join("external-edit.txt");
    fs::write(&external_edit, b"not owned by this journal").expect("write external edit");
    let drift = lifecycle
        .repair_resume(workspace.path(), journal_id)
        .expect_err("resume must refuse an edited applied record");
    assert!(matches!(
        drift,
        MissionError::Validation(_, diagnostics)
            if diagnostics[0].code == "repair-resume-drift"
    ));
    assert_eq!(
        lifecycle
            .get(mission_ids[1])
            .expect("second remains blocked")
            .bundle
            .manifest
            .revision,
        3
    );
    fs::remove_file(external_edit).expect("remove test-owned external edit");

    fs::remove_dir_all(&blocked_snapshot).expect("remove test-owned conflict");
    let resumed = lifecycle
        .repair_resume(workspace.path(), journal_id)
        .expect("resume repair");
    assert_eq!(resumed.status, "applied");
    let journal = resumed.journal.expect("completed journal");
    assert!(
        journal
            .records
            .iter()
            .all(|record| record.state == RepairRecordState::Applied)
    );
    for mission_id in mission_ids {
        assert_eq!(
            lifecycle
                .get(mission_id)
                .expect("repaired record")
                .bundle
                .manifest
                .revision,
            4
        );
    }
}

#[test]
fn repair_rollback_records_an_unchanged_failed_record_as_durable_compensation() {
    let workspace = TempDir::new().expect("workspace");
    let lifecycle = MissionLifecycle::open(workspace.path());
    let mission_id = divergent_mission(&lifecycle, "Unchanged failed repair record");
    let (request, proposal_id) = approved_request(&lifecycle, workspace.path(), &[mission_id]);
    let preview = lifecycle
        .repair_preview(workspace.path(), request)
        .expect("approved preview");
    let candidate = preview
        .candidate_overlay
        .get(&mission_id.to_string())
        .cloned()
        .expect("repair candidate");
    let before_payload_fingerprint = preview.records[0]
        .payload_fingerprint
        .clone()
        .expect("before payload fingerprint");
    let before_revision = preview.records[0]
        .observed_revision
        .expect("before revision");
    let journal_id = Uuid::from_u128(0x7a2b_2504_6555_4ca3_9f22_1c8d_4f5a_7b91);
    let mission_dir = lifecycle.store.root.join(mission_id.to_string());
    let snapshot = lifecycle.repair_snapshot_path(journal_id, mission_id);
    copy_tree(&mission_dir, &snapshot).expect("create durable before-state snapshot");
    let journal = RepairJournal {
        journal_id,
        source_root: workspace.path().to_path_buf(),
        proposal_id,
        state: RepairJournalState::Partial,
        records: vec![RepairJournalRecord {
            mission_id,
            before_revision,
            after_revision: Some(candidate.predicted_revision),
            before_payload_fingerprint: before_payload_fingerprint.clone(),
            after_payload_fingerprint: Some(candidate.predicted_payload_fingerprint.clone()),
            rollback_revision: None,
            rollback_payload_fingerprint: None,
            snapshot: Some(snapshot),
            state: RepairRecordState::Failed,
            diagnostic: Some("forced failure before authority write".into()),
            candidate: Some(candidate),
        }],
    };
    lifecycle
        .persist_repair_journal(&journal, true)
        .expect("persist failed repair journal");
    let before_payload = fingerprint_files(&mission_dir).expect("before payload");

    let rolled_back = lifecycle
        .repair_rollback(workspace.path(), journal_id)
        .expect("rollback unchanged record");
    assert_eq!(rolled_back.status, "rolled-back");
    assert_eq!(
        fingerprint_files(&mission_dir).expect("unchanged payload"),
        before_payload
    );
    let record = &rolled_back
        .journal
        .expect("durable rollback journal")
        .records[0];
    assert_eq!(record.state, RepairRecordState::RolledBack);
    assert_eq!(
        record.rollback_payload_fingerprint.as_deref(),
        Some(before_payload_fingerprint.as_str())
    );

    let repeated = lifecycle
        .repair_rollback(workspace.path(), journal_id)
        .expect("repeat rollback read-back");
    assert_eq!(repeated.status, "rolled-back");
}

#[test]
fn concurrent_repair_resumes_preserve_durable_snapshot_progress() {
    use std::sync::{Arc, Barrier};

    let workspace = TempDir::new().expect("workspace");
    let lifecycle = MissionLifecycle::open(workspace.path());
    let mission_id = divergent_mission(&lifecycle, "Concurrent repair resume");
    let mission_dir = lifecycle.store.root.join(mission_id.to_string());
    fs::write(
        mission_dir.join("payload-padding.bin"),
        vec![b'x'; 4 * 1024 * 1024],
    )
    .expect("write payload to overlap concurrent resumes");
    let (request, proposal_id) = approved_request(&lifecycle, workspace.path(), &[mission_id]);
    let preview = lifecycle
        .repair_preview(workspace.path(), request)
        .expect("approved preview");
    let candidate = preview
        .candidate_overlay
        .get(&mission_id.to_string())
        .cloned()
        .expect("repair candidate");
    let before = &preview.records[0];
    let journal_id = Uuid::from_u128(0x9b3c_2504_6555_4ca3_9f22_1c8d_4f5a_7b92);
    let journal = RepairJournal {
        journal_id,
        source_root: workspace.path().to_path_buf(),
        proposal_id,
        state: RepairJournalState::Partial,
        records: vec![RepairJournalRecord {
            mission_id,
            before_revision: before.observed_revision.expect("before revision"),
            after_revision: Some(candidate.predicted_revision),
            before_payload_fingerprint: before
                .payload_fingerprint
                .clone()
                .expect("before payload fingerprint"),
            after_payload_fingerprint: Some(candidate.predicted_payload_fingerprint.clone()),
            rollback_revision: None,
            rollback_payload_fingerprint: None,
            snapshot: None,
            state: RepairRecordState::Pending,
            diagnostic: None,
            candidate: Some(candidate),
        }],
    };
    lifecycle
        .persist_repair_journal(&journal, true)
        .expect("persist pending repair journal");
    let lock_path = lifecycle
        .repair_journal_path(journal_id)
        .with_file_name(format!(".{journal_id}.lock"));
    fs::write(lock_path, b"").expect("leave the stable lock file from a prior process");

    let barrier = Arc::new(Barrier::new(3));
    let root = workspace.path().to_path_buf();
    let workers = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let root = root.clone();
            std::thread::spawn(move || {
                let lifecycle = MissionLifecycle::open(&root);
                barrier.wait();
                lifecycle.repair_resume(&root, journal_id)
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for worker in workers {
        let response = worker
            .join()
            .expect("resume worker")
            .expect("concurrent resume response");
        assert_eq!(response.status, "applied");
    }

    let journal = lifecycle
        .repair_journal(workspace.path(), journal_id)
        .expect("read completed journal");
    assert_eq!(journal.state, RepairJournalState::Applied);
    let record = &journal.records[0];
    assert_eq!(record.state, RepairRecordState::Applied);
    let snapshot = record.snapshot.as_ref().expect("durable snapshot pointer");
    assert!(snapshot.is_dir(), "snapshot pointer remains recoverable");
    assert_eq!(
        payload_fingerprint(&fingerprint_files(&mission_dir).expect("repaired payload")),
        record
            .after_payload_fingerprint
            .as_deref()
            .expect("after fingerprint")
    );
}

fn divergent_mission(lifecycle: &MissionLifecycle, title: &str) -> Uuid {
    let draft = lifecycle
        .create_draft(title.to_owned(), "Verify partial repair recovery.".into())
        .expect("create typed draft");
    let mission_id = draft.mission_id;
    let draft = TypedDraft {
        waypoints: vec![TypedWaypoint {
            waypoint_id: "repair".into(),
            title: "Repair record".into(),
            prompt: "Apply one durable repair.".into(),
            part: PartSource {
                waypoint_id: "repair".into(),
                document: Document {
                    blocks: vec![Block::Paragraph {
                        inlines: vec![Inline::Text {
                            text: "Typed repair fixture.".into(),
                        }],
                    }],
                },
            },
            validation: vec![ValidationGate {
                command: "cargo test".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "Record repair fixture.".into(),
            },
            ..TypedWaypoint::default()
        }],
        ..draft
    };
    let draft = lifecycle
        .save_draft(mission_id, draft, 1)
        .expect("save typed source");
    lifecycle
        .publish_draft(mission_id, draft, 2)
        .expect("publish typed source");

    let source_path = lifecycle
        .store
        .root
        .join(mission_id.to_string())
        .join("source.json");
    let mut source: serde_json::Value =
        serde_json::from_slice(&fs::read(&source_path).expect("read typed source"))
            .expect("parse typed source");
    source["revision"] = json!(2);
    fs::write(
        source_path,
        serde_json::to_vec_pretty(&source).expect("serialize divergent source"),
    )
    .expect("create controlled revision divergence");
    mission_id
}

fn approved_request(
    lifecycle: &MissionLifecycle,
    workspace: &Path,
    mission_ids: &[Uuid],
) -> (RepairRequest, String) {
    let request = RepairRequest {
        source_root: workspace.to_path_buf(),
        policy: REPAIR_POLICY.into(),
        records: mission_ids
            .iter()
            .map(|mission_id| RepairRecordRequest {
                mission_id: *mission_id,
                expected_revision: lifecycle
                    .get(*mission_id)
                    .expect("accepted record")
                    .bundle
                    .manifest
                    .revision,
                accepted_sha256: String::new(),
                source_sha256: String::new(),
                payload_fingerprint: String::new(),
            })
            .collect(),
    };
    let discovery = lifecycle
        .repair_preview(workspace, request.clone())
        .expect("read-only preview");
    let mut approved = request;
    for (selection, observed) in approved.records.iter_mut().zip(discovery.records) {
        selection.expected_revision = observed.observed_revision.expect("observed revision");
        selection.accepted_sha256 = observed.observed_accepted_sha256.expect("accepted hash");
        selection.source_sha256 = observed.observed_source_sha256.expect("source hash");
        selection.payload_fingerprint = observed.payload_fingerprint.expect("payload fingerprint");
    }
    let preview = lifecycle
        .repair_preview(workspace, approved.clone())
        .expect("approved preview");
    assert_eq!(preview.status, "ready");
    (
        approved,
        preview
            .planned_journal_identity
            .expect("planned journal identity"),
    )
}
