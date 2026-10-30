use mission_api::{
    ControlEvent, ControlKind, ControlTarget, LIFECYCLE_JOURNAL_VERSION, LifecycleError,
    LifecycleJournal, LifecycleState,
};
use uuid::Uuid;

#[test]
fn schema_failures_keep_a_schema_diagnostic_code() {
    let error = LifecycleError::from(memory_kernel::lifecycle::LifecycleError::InvalidSchema(
        "invalid JSON schema".into(),
    ));
    assert_eq!(error.code(), "invalid_lifecycle_schema");
}

#[test]
fn exhausted_journal_sequence_is_an_explicit_error_not_a_panic() {
    let journal = LifecycleJournal {
        version: LIFECYCLE_JOURNAL_VERSION,
        events: vec![ControlEvent {
            seq: u64::MAX,
            prior_seq: 0,
            op_id: Uuid::new_v4(),
            target: ControlTarget::Mission,
            event: ControlKind::Pause,
            prior_state: LifecycleState::Active,
            evidence: None,
        }],
    };
    assert!(journal.projection().is_ok());
    let error = journal
        .next_event(
            ControlTarget::Mission,
            ControlKind::Resume,
            LifecycleState::Paused,
            Uuid::new_v4(),
            None,
        )
        .unwrap_err();
    assert_eq!(error.code(), "illegal_transition");
    assert!(error.to_string().contains("sequence exhausted"));
}

#[test]
fn last_available_sequence_can_be_used_once() {
    let mut journal = LifecycleJournal {
        version: LIFECYCLE_JOURNAL_VERSION,
        events: vec![ControlEvent {
            seq: u64::MAX - 1,
            prior_seq: 0,
            op_id: Uuid::new_v4(),
            target: ControlTarget::Mission,
            event: ControlKind::Pause,
            prior_state: LifecycleState::Active,
            evidence: None,
        }],
    };
    let event = journal
        .next_event(
            ControlTarget::Mission,
            ControlKind::Resume,
            LifecycleState::Paused,
            Uuid::new_v4(),
            None,
        )
        .unwrap();
    assert_eq!(event.seq, u64::MAX);
    journal.events.push(event);
    assert_eq!(journal.projection().unwrap().last_seq, u64::MAX);
    assert!(
        journal
            .next_event(
                ControlTarget::Mission,
                ControlKind::Pause,
                LifecycleState::Active,
                Uuid::new_v4(),
                None,
            )
            .is_err()
    );
}
