use super::*;

pub(crate) fn event(
    seq: u64,
    kind: ControlKind,
    target: ControlTarget,
    prior: LifecycleState,
) -> ControlEvent {
    ControlEvent {
        seq,
        op_id: Uuid::new_v4(),
        target,
        prior_seq: seq - 1,
        event: kind,
        prior_state: prior,
        evidence: (kind == ControlKind::Complete).then(|| "evidence".to_string()),
    }
}

pub(crate) fn wp(id: &str) -> ControlTarget {
    ControlTarget::Waypoint {
        waypoint_id: id.into(),
    }
}

pub(crate) fn journal(events: Vec<ControlEvent>) -> LifecycleJournal {
    LifecycleJournal {
        version: LIFECYCLE_JOURNAL_VERSION,
        events,
    }
}

// MN-W1-01
