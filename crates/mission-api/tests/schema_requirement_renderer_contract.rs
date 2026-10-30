use mission_api::*;
use serde_json::json;
use spec_api::{AcceptanceCriterion, CriterionArtifact, SpecManifest};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use ticket_api::model::ticket::TicketManifest;
use uuid::Uuid;

#[path = "schema_requirement_renderer_contract/support.rs"]
mod support;
use support::*;

#[path = "schema_requirement_renderer_contract/canonical_criteria.rs"]
mod canonical_criteria;
#[path = "schema_requirement_renderer_contract/renderer_contracts.rs"]
mod renderer_contracts;
