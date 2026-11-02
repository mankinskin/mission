use super::*;

#[test]
fn handoff_checks_both_sessions_and_recovers_destination_first_pin_transfer() {
    const DEST: &str = "22222222-2222-4222-8222-222222222222";
    let f = linked();
    f.approve("handoff", "handoff-event");
    f.session
        .init_runtime_context(session_api::SessionRuntimeInitRequest {
            session_id: Some(DEST.into()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    assert!(
        host_hook::run(
            &f.ws,
            DEST,
            serde_json::json!({
                "hook_event_name": "UserPromptSubmit", "prompt": "Continue in this Session",
                "message_id": "destination-active",
            })
        )
        .status
        .success()
    );
    let adapter = WorkspaceLifecycle::open(&f.ws);
    let req = request(&f, "transfer active binding");
    let urn = format!("ce://default/mission/{}", f.id);
    // Simulate interruption after publishing the destination copy.
    let lifecycle = MissionLifecycle::open(&f.ws);
    let tx = lifecycle.begin_approval_transaction(f.id).unwrap();
    let mut runtime = tx.control_state().unwrap();
    runtime.pending_handoff = Some(PendingHandoff {
        operation_id: req.op_id,
        source: SESSION.into(),
        destination: DEST.into(),
        reason: req.evidence.clone().unwrap(),
        expected_control_sequence: req.expected_control_sequence,
    });
    tx.write_control_state(&mut runtime, 0).unwrap();
    drop(tx);
    f.session
        .pin_runtime_entity(
            DEST,
            &urn,
            Some(format!("mission-handoff/{}", req.op_id)),
            Some("recover destination copy".into()),
        )
        .unwrap();
    assert_eq!(f.code(EntryPath::Dispatch), "cross_store_pending");
    adapter.handoff_session(f.id, SESSION, DEST, &req).unwrap();
    assert_eq!(state(&f).active_session.as_deref(), Some(DEST));
    assert!(state(&f).pending_handoff.is_none());
    let source = f.session.read_runtime_context(SESSION).unwrap();
    let destination = f.session.read_runtime_context(DEST).unwrap();
    assert!(!source.pinned_entities.iter().any(|p| p.urn == urn));
    assert!(destination.pinned_entities.iter().any(|p| p.urn == urn));
    assert_eq!(
        TicketStore::open(&f.ws)
            .unwrap()
            .get(&f.ticket)
            .unwrap()
            .extra["state"],
        "ready"
    );
    assert!(
        adapter
            .handoff_session(f.id, SESSION, DEST, &request(&f, "bad owner"))
            .is_err()
    );
    assert!(state(&f).pending_handoff.is_none());
}
