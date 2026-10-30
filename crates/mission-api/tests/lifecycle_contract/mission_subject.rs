use super::*;

struct Stub;
impl LifecycleSubject for Stub {
    type Identity = WaypointIdentity;
    type Snapshot = LifecycleSnapshot;
    type Error = LifecycleError;

    fn provider_name(&self) -> &str {
        "stub"
    }

    fn lifecycle_error(error: memory_kernel::lifecycle::LifecycleError) -> Self::Error {
        error.into()
    }
}
impl PauseResumeCapability for Stub {}
impl ReopenCapability for Stub {}

#[test]
fn unsupported_capabilities_return_explicit_errors() {
    let identity = WaypointIdentity::Ticket {
        ticket_id: Uuid::new_v4(),
    };
    let request = ControlRequest {
        op_id: Uuid::new_v4(),
        expected_control_sequence: 0,
        evidence: None,
    };
    let error = PauseResumeCapability::pause(&Stub, &identity, &request).unwrap_err();
    assert_eq!(error.code(), "unsupported_capability");
    assert_eq!(
        error,
        LifecycleError::UnsupportedCapability {
            capability: Capability::PauseResume,
            provider: "stub".into()
        }
    );
    assert!(PauseResumeCapability::resume(&Stub, &identity, &request).is_err());
    assert_eq!(
        ReopenCapability::reopen(&Stub, &identity, &request)
            .unwrap_err()
            .to_string(),
        "unsupported_capability: reopen is not supported by stub"
    );
}

pub(super) fn project(facts: &MissionFacts, control: &TargetControl) -> MissionProjection {
    project_mission(facts, control).unwrap()
}

// Review fix 1: completion is validated against required outcomes.
#[test]
fn completion_event_is_validated_against_required_outcomes() {
    let target = ControlTarget::Mission;
    let complete = journal(vec![event(
        1,
        ControlKind::Complete,
        target.clone(),
        Active,
    )])
    .projection()
    .unwrap()
    .target(&target);
    let facts = |waypoints| MissionFacts {
        waypoints,
        required_entry_unavailable: false,
    };
    for bad in [
        vec![Completed, Planned],
        vec![Active],
        vec![Completed, Cancelled],
        vec![Paused],
        vec![],
    ] {
        let error = project_mission(&facts(bad.clone()), &complete).unwrap_err();
        assert_eq!(error.code(), "control_history_invalid", "{bad:?}");
    }
    let ok = project_mission(&facts(vec![Completed, Completed]), &complete).unwrap();
    assert_eq!((ok.state, ok.result_review_needed), (Completed, false));
}

// Review fix 2: unsatisfied required entry (cancelled/blocked) blocks the Mission.
#[test]
fn cancelled_or_blocked_required_entry_blocks_mission_in_api_projection() {
    let none = TargetControl::default();
    for first in [Cancelled, Blocked] {
        let projected = project_mission(
            &MissionFacts {
                waypoints: vec![Completed, first, Planned],
                required_entry_unavailable: false,
            },
            &none,
        )
        .unwrap();
        assert_eq!(projected.state, Blocked, "{first:?}");
    }
    // A cancelled entry is never eligible work.
    let eligibility = ExecutionEligibility::effective(Blocked, &none);
    assert!(!eligibility.is_eligible());
}

// Review fix 3: journal targets must belong to the Mission.
#[test]
fn journal_waypoint_targets_are_validated_against_membership() {
    let known = ["a".to_string()].into_iter().collect();
    let ok = journal(vec![
        event(1, ControlKind::Pause, wp("a"), Active),
        event(2, ControlKind::Cancel, ControlTarget::Mission, Active),
    ]);
    ok.validate_waypoint_targets(&known).unwrap();
    let ghost = journal(vec![event(1, ControlKind::Pause, wp("ghost"), Active)]);
    let error = ghost.validate_waypoint_targets(&known).unwrap_err();
    assert_eq!(error.code(), "control_history_invalid");
    assert!(error.to_string().contains("ghost"));
}

// Review fix 4: the module path is public.
#[test]
fn lifecycle_module_path_is_public_and_reexports_are_retained() {
    let via_module: mission_api::lifecycle::LifecycleState =
        mission_api::lifecycle::LifecycleState::Planned;
    let via_root: mission_api::LifecycleState = via_module;
    assert_eq!(via_root.as_str(), "planned");
    assert_eq!(mission_api::lifecycle::LIFECYCLE_JOURNAL_VERSION, 1);
}
