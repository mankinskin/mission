use mission_api::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[path = "revision_history_approval_event_contract/support.rs"]
mod support;
use support::*;

#[path = "revision_history_approval_event_contract/approval_events.rs"]
mod approval_events;
#[path = "revision_history_approval_event_contract/migration_history.rs"]
mod migration_history;
#[path = "revision_history_approval_event_contract/state_contract.rs"]
mod state_contract;
