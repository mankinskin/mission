//! W2 (MN-W2-01..05) behavior tests for Session input intake and managed Missions.

use std::path::{Path, PathBuf};

use mission::intake::{
    GeneratedPlanState, IntakeError, IntakeStep, ManagedIntake, ManagedIntakeRequest, find,
    submit_and_create, submit_and_create_with_fault,
};
use session_api::{SessionError, SessionStoreConfig};
use uuid::Uuid;

const SESSION: &str = "11111111-1111-4111-8111-111111111111";
const ABC_DIGEST: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

fn reserved_mission(f: &Fixture, request_id: &str) -> Uuid {
    f.session()
        .read_intake(SESSION, request_id)
        .unwrap()
        .linkage
        .unwrap()
        .mission_id
}

fn pending_subset(error: IntakeError) -> (Option<Uuid>, Vec<IntakeStep>) {
    match error {
        IntakeError::Pending(receipt) => (receipt.mission_id, receipt.persisted),
        other => panic!("expected pending, got {other:?}"),
    }
}

#[path = "session_intake/support.rs"]
mod support;
use support::{Fixture, fixture};

#[path = "session_intake/fault_recovery.rs"]
mod fault_recovery;
#[path = "session_intake/managed_copy.rs"]
mod managed_copy;
#[path = "session_intake/replay_and_store_safety.rs"]
mod replay_and_store_safety;
