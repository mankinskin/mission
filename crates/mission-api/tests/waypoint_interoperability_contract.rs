use mission_api::*;
use serde_json::json;
use spec_api::{AcceptanceCriterion, SpecManifest, SpecStore};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use test_api::{TestStoreConfig, ValidationExecution};
use ticket_api::{model::ticket::TicketManifest, storage::ticket_fs::TicketFs};
use uuid::Uuid;

#[path = "waypoint_interoperability_contract/support.rs"]
mod support;
use support::*;

#[path = "waypoint_interoperability_contract/native_migration.rs"]
mod native_migration;
#[path = "waypoint_interoperability_contract/reference_edge_cases.rs"]
mod reference_edge_cases;
#[path = "waypoint_interoperability_contract/reference_resolution.rs"]
mod reference_resolution;
#[path = "waypoint_interoperability_contract/waypoint_basics.rs"]
mod waypoint_basics;
