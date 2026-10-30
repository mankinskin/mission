//! Fixture ingress exercises plumbing, not human identity.
#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/host_hook.rs"]
mod host_hook;
use approval_fixture::{Fx, SESSION, linked};
use mission::MissionLifecycle;
use mission::approval::{ApprovalStep, EntryPath};
use session_api::{RequesterDecisionRef, SessionRuntimeInitRequest};
use std::{
    fs,
    sync::{Arc, mpsc},
    time::Duration,
};
const SESSION2: &str = "22222222-2222-4222-8222-222222222222";

fn decide(f: &Fx, session: &str, challenge: &str, event: &str) -> RequesterDecisionRef {
    let prepared = f
        .approval()
        .prepare_challenge(f.id, session, challenge)
        .unwrap();
    assert!(host_hook::run(&f.ws, session, serde_json::json!({
        "hook_event_name": "UserPromptSubmit", "prompt": prepared.approve_line, "message_id": event,
    })).status.success());
    f.session
        .find_requester_decision_for_challenge(session, challenge)
        .unwrap()
        .unwrap()
        .reference
}
#[test]
fn unconsumed_grant_stays_stale_after_edit_and_fresh_challenge_recovers() {
    let f = linked();
    f.reviewed();
    let reference = decide(&f, SESSION, "c1", "evt-1");
    f.approval()
        .accept_decision_with_fault(f.id, &reference, Some(ApprovalStep::AfterGrant))
        .unwrap_err();
    f.edit_b("semantic", "b changed");
    assert_eq!(
        f.approval()
            .accept_decision(f.id, &reference)
            .unwrap_err()
            .code(),
        "approval_stale"
    );
    assert!(
        f.session
            .read_requester_consumption(SESSION, "c1")
            .unwrap()
            .is_none()
    );
    assert!(f.approval().grants(f.id).unwrap().is_empty());
    assert_eq!(
        f.approval().pending(f.id).unwrap()[0].stage,
        "disposition-unconsumed"
    );
    assert_eq!(f.code(EntryPath::Activation), "approval_stale");
    f.reviewed();
    let fresh = decide(&f, SESSION, "c2", "evt-2");
    let grant = f
        .approval()
        .accept_decision(f.id, &fresh)
        .unwrap()
        .grant
        .unwrap();
    assert_eq!(
        f.approval()
            .check_entry(f.id, EntryPath::Activation)
            .unwrap(),
        grant
    );
}
#[test]
fn acceptance_and_semantic_edit_serialize_on_publication_lock() {
    let f = Arc::new(linked());
    f.reviewed();
    let reference = decide(&f, SESSION, "c1", "evt-1");
    let lifecycle = MissionLifecycle::open(&f.ws);
    let held = lifecycle.begin_approval_transaction(f.id).unwrap();
    let (rt, rr) = mpsc::channel();
    let (dt, dr) = mpsc::channel::<(&'static str, Result<(), &'static str>)>();
    {
        let (f, ready, done) = (f.clone(), rt.clone(), dt.clone());
        std::thread::spawn(move || {
            ready.send(()).unwrap();
            let result = f
                .approval()
                .accept_decision(f.id, &reference)
                .map(|_| ())
                .map_err(|e| e.code());
            let _ = done.send(("accept", result));
        });
    }
    {
        let (f, ready, done) = (f.clone(), rt.clone(), dt.clone());
        std::thread::spawn(move || {
            ready.send(()).unwrap();
            f.edit_b("race", "b raced");
            let _ = done.send(("edit", Ok(())));
        });
    }
    for _ in 0..2 {
        rr.recv_timeout(Duration::from_secs(10)).unwrap();
    }
    assert!(dr.recv_timeout(Duration::from_millis(300)).is_err());
    drop(held);
    let mut accept = None;
    for _ in 0..2 {
        let (name, result) = dr.recv_timeout(Duration::from_secs(60)).unwrap();
        if name == "accept" {
            accept = Some(result);
        } else {
            assert_eq!(result, Ok(()));
        }
    }
    match accept.unwrap() {
        Ok(()) => assert_eq!(f.code(EntryPath::Activation), "approval_stale"),
        Err(code) => {
            assert_eq!(code, "approval_stale");
            assert!(f.approval().grants(f.id).unwrap().is_empty());
            assert_eq!(f.code(EntryPath::Activation), "approval_required");
        }
    }
}
#[test]
fn tampered_journaled_session_challenge_root_refuses_every_entry() {
    let f = linked();
    f.approve("c1", "evt-1");
    f.approval().check_entry(f.id, EntryPath::Resume).unwrap();
    let path = f
        .session
        .root
        .join("sessions")
        .join(SESSION)
        .join("requester/c1/challenge.json");
    let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["mission_root"] = "/elsewhere/ws".into();
    fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    for entry in [
        EntryPath::Activation,
        EntryPath::Dispatch,
        EntryPath::Resume,
    ] {
        assert!(matches!(
            f.code(entry),
            "approval_conflict" | "approval_authority_unavailable"
        ));
    }
}
#[test]
fn successor_needs_fresh_challenge_after_predecessor_finishes() {
    let f = linked();
    let first = f.approve("c1", "evt-1");
    f.approval()
        .check_entry(f.id, EntryPath::Activation)
        .unwrap();
    f.session
        .finish_workflow(SESSION, vec![], vec![], None)
        .unwrap();
    assert_eq!(f.code(EntryPath::Activation), "approval_required");
    assert!(
        f.approval()
            .prepare_challenge(f.id, SESSION, "c-late")
            .is_err()
    );
    f.session
        .init_runtime_context(SessionRuntimeInitRequest {
            session_id: Some(SESSION2.into()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    let fresh = decide(&f, SESSION2, "c2", "evt-2");
    let grant = f
        .approval()
        .accept_decision(f.id, &fresh)
        .unwrap()
        .grant
        .unwrap();
    assert_ne!(fresh, first);
    assert_eq!(grant.decision.session_id, SESSION2);
    assert_eq!(
        f.approval()
            .check_entry(f.id, EntryPath::Activation)
            .unwrap(),
        grant
    );
}
