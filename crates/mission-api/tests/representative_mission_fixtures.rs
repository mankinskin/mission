#[path = "../src/lib.rs"]
mod mission_api;

use mission_api::*;
use serde_json::json;
use spec_api::{AcceptanceCriterion, SpecManifest, SpecStore};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use test_api::{TestStoreConfig, ValidationExecution};
use ticket_api::{model::ticket::TicketManifest, storage::ticket_fs::TicketFs};
use uuid::Uuid;

#[path = "representative_mission_fixtures/support/mod.rs"]
mod support;
use support::*;

#[path = "representative_mission_fixtures/bootstrap.rs"]
mod bootstrap;
#[path = "representative_mission_fixtures/event_tree.rs"]
mod event_tree;
#[path = "representative_mission_fixtures/fixture_regeneration.rs"]
mod fixture_regeneration;
#[path = "representative_mission_fixtures/output_contract.rs"]
mod output_contract;
#[path = "representative_mission_fixtures/reference_validation.rs"]
mod reference_validation;
