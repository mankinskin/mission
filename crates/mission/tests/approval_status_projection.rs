#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;

use approval_fixture::{Fx, SESSION, linked};
use mission::approval::{EntryPath, ReviewKind, ReviewVerdict};
use mission::{MissionLifecycle, MissionStore, Operation, execute_cli, execute_mcp};
use std::{collections::BTreeMap, fs};
use ticket_api::storage::TicketStore;

fn publish_status_aware(f: &Fx) {
    let store = MissionStore::open(&f.ws);
    let mut accepted = store.get(f.id).unwrap();
    let revision = accepted.bundle.manifest.revision;
    accepted.bundle.manifest.revision += 1;
    let bundle = f.ws.join("status-bundle");
    fs::create_dir_all(&bundle).unwrap();
    accepted.bundle.manifest.waypoint_files = accepted
        .bundle
        .waypoints
        .iter()
        .map(|waypoint| {
            let filename = format!("{}.json", waypoint.waypoint_id);
            fs::write(
                bundle.join(&filename),
                serde_json::to_vec_pretty(waypoint).unwrap(),
            )
            .unwrap();
            filename
        })
        .collect();
    let manifest_path = bundle.join("manifest.json");
    fs::write(
        &manifest_path,
        serde_json::to_vec_pretty(&accepted.bundle.manifest).unwrap(),
    )
    .unwrap();
    let result = execute_cli(
        &f.ws,
        Operation::Publish {
            mission_id: f.id,
            manifest_path,
            expected_current_revision: revision,
        },
    );
    assert_eq!(result.status, "ok", "{:?}", result.diagnostics);
}

fn update_status(f: &Fx, state: &str) {
    TicketStore::open(&f.ws)
        .unwrap()
        .update(&f.ticket, BTreeMap::new(), None, Some(state), None, None)
        .unwrap();
}

fn generated(f: &Fx) -> std::path::PathBuf {
    f.ws.join(".workflow-tools/mission/missions")
        .join(f.id.to_string())
        .join("generated/ROADMAP.md")
}

#[test]
fn typed_publication_agrees_with_check_generated_and_review_guard() {
    let f = linked();
    let status = execute_mcp(&f.ws, Operation::CheckGenerated { mission_id: f.id });
    assert_eq!(status.status, "ok", "{:?}", status.diagnostics);
    f.reviewed();
    f.approval()
        .prepare_challenge(f.id, SESSION, "typed")
        .unwrap();
}

#[test]
fn status_aware_publication_agrees_with_check_generated_and_review_guard() {
    let f = linked();
    publish_status_aware(&f);
    let status = execute_cli(&f.ws, Operation::CheckGenerated { mission_id: f.id });
    assert_eq!(status.status, "ok", "{:?}", status.diagnostics);
    f.reviewed();
    f.approval()
        .prepare_challenge(f.id, SESSION, "status-aware")
        .unwrap();
}

#[test]
fn ticket_change_before_review_or_challenge_requires_fresh_publication() {
    for status_aware in [false, true] {
        for reviewed in [false, true] {
            let f = linked();
            if status_aware {
                publish_status_aware(&f);
            }
            if reviewed {
                f.reviewed();
            }
            let before = fs::read(generated(&f)).unwrap();
            update_status(&f, "planning");
            let status = execute_cli(&f.ws, Operation::CheckGenerated { mission_id: f.id });
            assert_eq!(status.status, "drift");
            if reviewed {
                assert!(
                    f.approval()
                        .prepare_challenge(f.id, SESSION, "stale")
                        .is_err()
                );
                assert!(
                    f.session
                        .read_requester_challenge(SESSION, "stale")
                        .is_err()
                );
            } else {
                assert!(
                    f.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Pass, "cold")
                        .is_err()
                );
            }
            assert_eq!(fs::read(generated(&f)).unwrap(), before);
            let lifecycle = MissionLifecycle::open(&f.ws);
            let draft = lifecycle.get_mission_source(f.id).unwrap();
            let revision = draft.revision;
            lifecycle.publish_draft(f.id, draft, revision).unwrap();
            assert_eq!(
                execute_cli(&f.ws, Operation::CheckGenerated { mission_id: f.id }).status,
                "ok"
            );
            f.approve("fresh", "fresh-event");
        }
    }
}

#[test]
fn ticket_change_after_challenge_blocks_acceptance_without_consuming_receipt() {
    let f = linked();
    f.reviewed();
    let prepared = f
        .approval()
        .prepare_challenge(f.id, SESSION, "stale-decision")
        .unwrap();
    assert!(
        host_hook::run(
            &f.ws,
            SESSION,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit", "prompt": prepared.approve_line,
                "message_id": "stale-decision-event",
            }),
        )
        .status
        .success()
    );
    let reference = f
        .session
        .find_requester_decision_for_challenge(SESSION, "stale-decision")
        .unwrap()
        .unwrap()
        .reference;
    update_status(&f, "planning");
    assert!(f.approval().accept_decision(f.id, &reference).is_err());
    assert!(f.approval().grants(f.id).unwrap().is_empty());
    assert!(
        f.session
            .read_requester_consumption(SESSION, "stale-decision")
            .unwrap()
            .is_none()
    );
}

#[test]
fn ticket_progress_after_approval_preserves_entry_and_reviewed_output() {
    for status_aware in [false, true] {
        let f = linked();
        if status_aware {
            publish_status_aware(&f);
        }
        f.approve("approved", "approved-event");
        let before = fs::read(generated(&f)).unwrap();
        for state in ["in-implementation", "in-review", "done"] {
            update_status(&f, state);
            let status = execute_mcp(&f.ws, Operation::CheckGenerated { mission_id: f.id });
            assert_eq!(status.status, "drift");
            for path in [
                EntryPath::Activation,
                EntryPath::Dispatch,
                EntryPath::Resume,
            ] {
                f.approval().check_entry(f.id, path).unwrap();
            }
            MissionStore::open(&f.ws)
                .verify_output_history(f.id)
                .unwrap();
            assert_eq!(fs::read(generated(&f)).unwrap(), before);
        }
    }
}

#[test]
fn missing_ticket_blocks_typed_publication_without_changing_the_output() {
    let f = linked();
    let lifecycle = MissionLifecycle::open(&f.ws);
    let draft = lifecycle.get_mission_source(f.id).unwrap();
    let revision = draft.revision;
    let before = fs::read(generated(&f)).unwrap();
    fs::remove_file(
        f.ws.join(".workflow-tools/ticket/tickets")
            .join(f.ticket.to_string())
            .join("ticket.toml"),
    )
    .unwrap();
    let error = lifecycle.publish_draft(f.id, draft, revision).unwrap_err();
    assert_eq!(
        error.diagnostics().unwrap()[0].code,
        "external-status-unresolved"
    );
    assert_eq!(
        lifecycle.get(f.id).unwrap().bundle.manifest.revision,
        revision
    );
    assert_eq!(fs::read(generated(&f)).unwrap(), before);
    assert!(
        f.review(ReviewKind::ColdWalkthrough, ReviewVerdict::Pass, "cold")
            .is_err()
    );
}

#[test]
fn ticket_progress_does_not_allow_modified_generated_content_at_entry() {
    let f = linked();
    f.approve("immutable", "immutable-event");
    update_status(&f, "in-implementation");
    f.approval()
        .check_entry(f.id, EntryPath::Activation)
        .unwrap();
    fs::write(generated(&f), "Unreviewed replacement").unwrap();
    for path in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert!(f.approval().check_entry(f.id, path).is_err());
    }
}
