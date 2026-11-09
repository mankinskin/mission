use crate::approval_fixture::Fx;
use mission::*;
use std::{collections::BTreeMap, sync::Arc};

#[path = "../../../mission-api/tests/provider_evidence/support.rs"]
pub mod provider;
pub use provider::TestProvider;

pub fn binding(f: &Fx, waypoint_id: &str, provider: &TestProvider) -> ProviderEvidenceBindingV4 {
    let plan = f.plan();
    let waypoint = plan
        .waypoints
        .iter()
        .find(|w| w.waypoint_id == waypoint_id)
        .unwrap();
    let contract = waypoint.execution_contract.as_ref().unwrap();
    let lifecycle = MissionLifecycle::open(&f.ws);
    let tx = lifecycle.begin_approval_transaction(f.id).unwrap();
    let intent = tx
        .effective_intent(&mission::intent::SessionIntentProvider::new(&f.ws).unwrap())
        .unwrap()
        .unwrap();
    let state = tx.control_state().unwrap();
    let mut prerequisites = BTreeMap::new();
    for dependency in &waypoint.depends_on {
        let completion = state
            .completions
            .iter()
            .rev()
            .find(|c| c.current && &c.waypoint_id == dependency)
            .unwrap();
        let resolved = provider
            .resolve(&ProviderEvidenceRefV4 {
                receipt: completion.result_ref.clone(),
            })
            .unwrap();
        let record: CompletionEvidenceV4 = serde_json::from_slice(&resolved.content).unwrap();
        prerequisites.insert(
            dependency.clone(),
            record
                .handover
                .map_or(resolved.content_sha256, |a| a.content_sha256),
        );
    }
    let mut binding = provider::binding(contract);
    binding.mission_id = f.id;
    binding.owning_root = f.ws.to_string_lossy().into_owned();
    let route = contract.provider_route.as_ref();
    binding.provider = route
        .map_or("mission-validation", |r| r.provider.as_str())
        .into();
    binding.provider_version = route.map_or("4", |r| r.version.as_str()).into();
    binding.route_id = route.map_or("automated-checks", |r| r.id.as_str()).into();
    binding.receipt_class = route
        .map_or("completion-v4", |r| r.receipt_class.as_str())
        .into();
    binding.environment_digest = provider
        .environment_digest(&binding.owning_root, &binding.profile)
        .unwrap();
    binding.source_digest = intent.baseline.source_sha256;
    binding.intent_chain_digest = intent.chain_sha256;
    binding.plan_digest = plan.plan_digest;
    binding.waypoint_id = waypoint_id.into();
    binding.prerequisite_artifact_digests = prerequisites;
    binding
}

pub fn put(f: &Fx, waypoint: &str, receipt: &str, provider: &TestProvider) -> CompletionEvidenceV4 {
    let plan = f.plan();
    let contract = plan
        .waypoints
        .iter()
        .find(|w| w.waypoint_id == waypoint)
        .unwrap()
        .execution_contract
        .as_ref()
        .unwrap();
    let record = provider::record(
        contract,
        binding(f, waypoint, provider),
        EvidencePurposeV4::Completion,
        receipt,
    );
    provider.put(&record);
    record
}

/// Reconstruct independent provider-owned fixture records for existing lifecycle
/// tests. This never changes the production dispatcher or its adapter registry.
pub fn adapter(f: &Fx) -> WorkspaceLifecycle {
    let provider = Arc::new(TestProvider::new());
    let plan = f.plan();
    for waypoint in &plan.waypoints {
        let state = MissionLifecycle::open(&f.ws)
            .begin_approval_transaction(f.id)
            .unwrap()
            .control_state()
            .unwrap();
        if waypoint.depends_on.iter().any(|d| {
            !state
                .completions
                .iter()
                .any(|c| c.current && &c.waypoint_id == d)
        }) {
            continue;
        }
        for evidence in waypoint.evidence.iter().filter(|e| e.current) {
            put(
                f,
                &waypoint.waypoint_id,
                &evidence.evidence_pointer,
                &provider,
            );
        }
    }
    WorkspaceLifecycle::open(&f.ws).with_test_evidence_provider(provider)
}
