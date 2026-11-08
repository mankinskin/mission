#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;

use approval_fixture::{Fx, SESSION, linked};
use mission::MissionStore;
use mission::approval::EntryPath;
use sha2::{Digest, Sha256};
use std::fs;

fn directory(f: &Fx) -> std::path::PathBuf {
    f.ws.join(".workflow-tools/mission/missions")
        .join(f.id.to_string())
}

fn rewrite_document_and_contract(f: &Fx) {
    let directory = directory(f);
    let bytes = b"substituted reviewed document";
    let archive = directory
        .join("history/states")
        .join(format!("revision-{}", f.plan().revision));
    let mut contract: serde_json::Value =
        serde_json::from_slice(&fs::read(directory.join("output-contract.json")).unwrap()).unwrap();
    let file = contract["files"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|file| file["path"] == "mission.md")
        .unwrap();
    file["sha256"] = format!("{:x}", Sha256::digest(bytes)).into();
    file["size_bytes"] = (bytes.len() as u64).into();
    for root in [&directory, &archive] {
        fs::write(root.join("mission.md"), bytes).unwrap();
        fs::write(
            root.join("output-contract.json"),
            serde_json::to_vec_pretty(&contract).unwrap(),
        )
        .unwrap();
    }
    MissionStore::open(&f.ws)
        .verify_output_history(f.id)
        .unwrap();
}

#[test]
fn inconsistent_planning_document_blocks_challenge_without_journaling() {
    let f = linked();
    f.reviewed();
    let directory =
        f.ws.join(".workflow-tools/mission/missions")
            .join(f.id.to_string());
    let journal = directory.join("approvals/journal");
    let count = fs::read_dir(&journal).unwrap().count();
    fs::write(directory.join("mission.md"), "unreviewed planning changes").unwrap();
    assert!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "drift")
            .is_err()
    );
    assert_eq!(fs::read_dir(journal).unwrap().count(), count);
    assert!(
        f.session
            .read_requester_challenge(SESSION, "drift")
            .is_err()
    );
}

#[test]
fn document_changed_after_challenge_blocks_acceptance_without_grant() {
    let f = linked();
    f.reviewed();
    let challenge = f
        .approval()
        .prepare_challenge(f.id, SESSION, "drift")
        .unwrap();
    assert!(
        host_hook::run(
            &f.ws,
            SESSION,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit", "prompt": challenge.approve_line,
                "message_id": "drift-decision",
            })
        )
        .status
        .success()
    );
    let reference = f
        .session
        .find_requester_decision_for_challenge(SESSION, "drift")
        .unwrap()
        .unwrap()
        .reference;
    let directory =
        f.ws.join(".workflow-tools/mission/missions")
            .join(f.id.to_string());
    fs::write(directory.join("mission.md"), "changed after challenge").unwrap();
    assert!(f.approval().accept_decision(f.id, &reference).is_err());
    assert!(f.approval().grants(f.id).unwrap().is_empty());
    assert!(
        f.session
            .read_requester_consumption(SESSION, "drift")
            .unwrap()
            .is_none()
    );
}

#[test]
fn document_changed_after_grant_blocks_every_entry_path() {
    let f = linked();
    f.approve("entry", "entry-decision");
    let directory =
        f.ws.join(".workflow-tools/mission/missions")
            .join(f.id.to_string());
    fs::write(directory.join("mission.md"), "changed after approval").unwrap();
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert!(f.approval().check_entry(f.id, path).is_err());
    }
}

#[test]
fn legitimate_review_challenge_and_grant_appends_preserve_output_integrity() {
    let f = linked();
    f.approve("healthy", "healthy-decision");
    MissionStore::open(&f.ws)
        .verify_output_history(f.id)
        .unwrap();
    f.approval()
        .check_entry(f.id, EntryPath::Activation)
        .unwrap();
}

#[test]
fn matching_rewritten_current_and_archived_contracts_cannot_reuse_review_or_approval() {
    for stage in ["review", "challenge", "grant"] {
        let f = linked();
        f.reviewed();
        let reference = if stage == "grant" {
            Some(f.approve("bound", "bound-event"))
        } else if stage == "challenge" {
            let challenge = f
                .approval()
                .prepare_challenge(f.id, SESSION, "bound")
                .unwrap();
            assert!(
                host_hook::run(
                    &f.ws,
                    SESSION,
                    serde_json::json!({
                        "hook_event_name": "UserPromptSubmit", "prompt": challenge.approve_line,
                        "message_id": "bound-event",
                    })
                )
                .status
                .success()
            );
            Some(
                f.session
                    .find_requester_decision_for_challenge(SESSION, "bound")
                    .unwrap()
                    .unwrap()
                    .reference,
            )
        } else {
            None
        };
        rewrite_document_and_contract(&f);
        match stage {
            "review" => assert!(
                f.approval()
                    .prepare_challenge(f.id, SESSION, "new")
                    .is_err()
            ),
            "challenge" => assert!(
                f.approval()
                    .accept_decision(f.id, reference.as_ref().unwrap())
                    .is_err()
            ),
            _ => assert!(
                f.approval()
                    .check_entry(f.id, EntryPath::Activation)
                    .is_err()
            ),
        }
    }
}

#[test]
fn altered_review_bytes_block_decision_and_entry_even_when_review_pair_still_passes() {
    for granted in [false, true] {
        let f = linked();
        let reference = if granted {
            f.approve("journal", "journal-event")
        } else {
            f.reviewed();
            let challenge = f
                .approval()
                .prepare_challenge(f.id, SESSION, "journal")
                .unwrap();
            assert!(
                host_hook::run(
                    &f.ws,
                    SESSION,
                    serde_json::json!({
                        "hook_event_name": "UserPromptSubmit", "prompt": challenge.approve_line,
                        "message_id": "journal-event",
                    })
                )
                .status
                .success()
            );
            f.session
                .find_requester_decision_for_challenge(SESSION, "journal")
                .unwrap()
                .unwrap()
                .reference
        };
        let path = directory(&f).join("approvals/journal/00000001.json");
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["entry"]["reviewer"] = "another-distinct-reviewer".into();
        fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
        if granted {
            assert!(f.approval().check_entry(f.id, EntryPath::Dispatch).is_err());
        } else {
            assert!(f.approval().accept_decision(f.id, &reference).is_err());
        }
    }
}

#[test]
fn incomplete_or_corrupt_output_states_block_challenges() {
    for mutation in [
        "missing-contract",
        "malformed-contract",
        "archive",
        "extra-file",
        "missing-ticket",
    ] {
        let f = linked();
        f.reviewed();
        let directory = directory(&f);
        match mutation {
            "missing-contract" => fs::remove_file(directory.join("output-contract.json")).unwrap(),
            "malformed-contract" => fs::write(directory.join("output-contract.json"), "{").unwrap(),
            "archive" => fs::write(
                directory.join("history/states/revision-2/mission.md"),
                "tampered",
            )
            .unwrap(),
            "extra-file" => fs::write(directory.join("unexpected.md"), "unreviewed").unwrap(),
            _ => fs::remove_dir_all(
                f.ws.join(".workflow-tools/ticket/tickets")
                    .join(f.ticket.to_string()),
            )
            .unwrap(),
        }
        assert!(
            f.approval()
                .prepare_challenge(f.id, SESSION, "invalid")
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn snapshotted_journal_prefix_and_strict_archives_remain_immutable() {
    for mutation in ["prefix", "gap", "malformed", "archived-append"] {
        let f = linked();
        f.reviewed();
        let lifecycle = mission::MissionLifecycle::open(&f.ws);
        let source = lifecycle.get_mission_source(f.id).unwrap();
        lifecycle
            .publish_draft(f.id, source, f.plan().revision)
            .unwrap();
        let directory = directory(&f);
        let journal = directory.join("approvals/journal");
        match mutation {
            "prefix" => {
                let path = journal.join("00000001.json");
                let mut record: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                record["entry"]["reviewer"] = "modified".into();
                fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
            }

            "gap" => fs::remove_file(journal.join("00000001.json")).unwrap(),
            "malformed" => fs::write(journal.join("00000003.json"), "{").unwrap(),
            _ => {
                let archive = directory
                    .join("history/states")
                    .join(format!("revision-{}", f.plan().revision));
                fs::copy(
                    journal.join("00000002.json"),
                    archive.join("approvals/journal/00000003.json"),
                )
                .unwrap();
            }
        }
        assert!(
            MissionStore::open(&f.ws)
                .verify_output_history(f.id)
                .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn legacy_unbound_reviews_remain_readable_but_cannot_issue_a_challenge() {
    let f = linked();
    f.reviewed();
    for entry in fs::read_dir(directory(&f).join("approvals/journal")).unwrap() {
        let path = entry.unwrap().path();
        let mut record: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        record["entry"]
            .as_object_mut()
            .unwrap()
            .remove("output_contract_binding");
        fs::write(path, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
    }
    assert_eq!(f.approval().reviews(f.id).unwrap().len(), 2);
    assert_eq!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "legacy")
            .unwrap_err()
            .code(),
        "approval_stale"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_output_is_rejected_before_challenge() {
    let f = linked();
    f.reviewed();
    let path = directory(&f).join("mission.md");
    let target = f.ws.join("untrusted.md");
    fs::rename(&path, &target).unwrap();
    std::os::unix::fs::symlink(&target, path).unwrap();
    assert!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "symlink")
            .is_err()
    );
}
