#[allow(dead_code)]
#[path = "support/approval_fixture.rs"]
mod approval_fixture;
#[path = "support/direct_transports.rs"]
mod direct_transports;
#[allow(dead_code)]
#[path = "support/host_hook.rs"]
mod host_hook;

use approval_fixture::Fx;
use direct_transports::{Mcp, cli, json};
use mission::{
    BaselineRequirementV4, IntentChangeV4, IntentEvidencePolicy, IntentOutcomeCategory,
    IntentReceiptReference, IntentSourceSpan, MissionLifecycle, MissionStore, WaypointWorkKind,
    approval::{
        GoalCoverageDispositionV4, GoalCoverageFindingV4, PlannedAcceptanceCheckV4,
        PlannedEvidenceClassV4, ReviewInput, ReviewKind, ReviewVerdict,
        TerminalOutcomeConclusionV4, TerminalOutcomeFindingV4,
    },
    intent::{IntentOperation, IntentOutput, SessionIntentProvider, execute_intent},
    intent_source_digest,
};
use session_api::HostPromptIngress;
use std::fs;
use uuid::Uuid;

#[path = "review_coverage/shared.rs"]
mod shared;
use shared::*;

#[path = "review_coverage/cases.rs"]
mod cases;
#[path = "review_coverage/policy.rs"]
mod policy;
#[path = "review_coverage/transports.rs"]
mod transports;
