use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use mission_api::{
    BundleManifest, CommitCheckpoint, MissionBundle, MissionError, MissionOutputInputs,
    MissionStore, SCHEMA_VERSION, Scope, ValidationGate, WaypointFragment, WaypointMode,
    WaypointState, validate_bundle,
};
use serde::Serialize;
use uuid::Uuid;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DossierMigrationReport {
    pub dry_run: bool,
    pub migrated: usize,
    pub would_migrate: usize,
    pub skipped: usize,
    pub blocked: usize,
    pub records: Vec<DossierMigrationRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DossierMigrationRecord {
    pub source_path: String,
    pub mission_id: Option<Uuid>,
    pub outcome: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug)]
struct ParsedWaypoint {
    source_label: usize,
    fragment: WaypointFragment,
    dependencies: Vec<usize>,
}

mod parse;
mod paths;
mod run;

use parse::*;
use paths::*;
pub use run::migrate_dossiers;
