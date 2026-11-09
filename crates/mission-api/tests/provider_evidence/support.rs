use mission_api::*;
use std::{collections::BTreeMap, fs, sync::Mutex};
use uuid::Uuid;

pub struct TestProvider {
    pub dir: tempfile::TempDir,
    pub records: Mutex<BTreeMap<String, (String, String)>>,
    pub preflight: Mutex<Option<ProviderEvidenceRefV4>>,
    pub environment: Mutex<String>,
}

impl TestProvider {
    pub fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            records: Mutex::new(BTreeMap::new()),
            preflight: Mutex::new(None),
            environment: Mutex::new(evidence_content_digest(b"fixture-environment")),
        }
    }

    pub fn put(&self, record: &CompletionEvidenceV4) -> ProviderEvidenceRefV4 {
        let content = serde_json::to_vec(record).unwrap();
        let filename = format!(
            "{}.json",
            evidence_content_digest(record.reference.receipt.as_bytes())
        );
        fs::write(self.dir.path().join(&filename), &content).unwrap();
        self.records.lock().unwrap().insert(
            record.reference.receipt.clone(),
            (filename, evidence_content_digest(&content)),
        );
        if record.purpose == EvidencePurposeV4::Preflight {
            *self.preflight.lock().unwrap() = Some(record.reference.clone());
        }
        record.reference.clone()
    }
}

impl ReadOnlyEvidenceProviderV4 for TestProvider {
    fn environment_digest(&self, _: &str, _: &str) -> Result<String, LifecycleError> {
        Ok(self.environment.lock().unwrap().clone())
    }
    fn preflight_receipt(
        &self,
        _: &ProviderEvidenceBindingV4,
    ) -> Result<ProviderEvidenceRefV4, LifecycleError> {
        self.preflight
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| control_error("provider_receipt_missing", "provider preflight absent"))
    }
    fn resolve(
        &self,
        reference: &ProviderEvidenceRefV4,
    ) -> Result<ResolvedProviderEvidenceV4, LifecycleError> {
        let records = self.records.lock().unwrap();
        let (filename, digest) = records
            .get(&reference.receipt)
            .ok_or_else(|| control_error("provider_receipt_missing", "opaque receipt not found"))?;
        Ok(ResolvedProviderEvidenceV4 {
            content: fs::read(self.dir.path().join(filename))
                .map_err(|e| control_error("provider_evidence_corrupt", e.to_string()))?,
            content_sha256: digest.clone(),
        })
    }
}

pub fn binding(contract: &WaypointExecutionContractV4) -> ProviderEvidenceBindingV4 {
    ProviderEvidenceBindingV4 {
        mission_id: Uuid::from_u128(1),
        owning_root: "/fixture/workspace".into(),
        provider: "fixture-validation".into(),
        provider_version: "4".into(),
        route_id: "fixture-route".into(),
        receipt_class: "checks-v4".into(),
        profile: contract.environment.profile.clone(),
        environment_digest: evidence_content_digest(b"fixture-environment"),
        source_digest: evidence_content_digest(b"source"),
        intent_chain_digest: evidence_content_digest(b"confirmed-intent"),
        plan_digest: evidence_content_digest(b"plan"),
        waypoint_id: "w1".into(),
        contract_digest: waypoint_execution_contract_digest(contract).unwrap(),
        prerequisite_artifact_digests: BTreeMap::new(),
    }
}

pub fn record(
    contract: &WaypointExecutionContractV4,
    binding: ProviderEvidenceBindingV4,
    purpose: EvidencePurposeV4,
    receipt: &str,
) -> CompletionEvidenceV4 {
    let conditions = if purpose == EvidencePurposeV4::Preflight {
        &contract.preconditions
    } else {
        &contract.postconditions
    };
    CompletionEvidenceV4 {
        schema_version: 4,
        reference: ProviderEvidenceRefV4 {
            receipt: receipt.into(),
        },
        purpose,
        binding,
        ready: true,
        validity: ProviderValidityV4::Current,
        condition_ids: conditions.iter().map(|c| c.id.clone()).collect(),
        checks: contract
            .checks
            .iter()
            .filter(|c| applicable_execution_check(contract, c, purpose))
            .map(|c| AutomatedCheckEvidenceV4 {
                check_id: c.id.clone(),
                check_digest: execution_check_digest(c).unwrap(),
                evidence_class: c.evidence_class,
                executed: 1,
                failed: 0,
                skipped: 0,
                mock: false,
            })
            .collect(),
        handover: None,
        diagnostic_reruns: 0,
        fix_justification: None,
    }
}
