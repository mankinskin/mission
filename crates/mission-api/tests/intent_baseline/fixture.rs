use mission_api::*;
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;

pub(super) struct Provider(pub(super) Vec<IntentReceiptV4>);

impl IntentReceiptProvider for Provider {
    fn confirmed_receipt(
        &self,
        _: Uuid,
        reference: &IntentReceiptReference,
    ) -> Result<IntentReceiptV4, MissionError> {
        self.0
            .iter()
            .find(|receipt| receipt.challenge_id == reference.challenge_id.to_string())
            .cloned()
            .ok_or_else(|| {
                MissionError::Validation(
                    1,
                    vec![Diagnostic {
                        code: "intent-confirmation-required",
                        source: "intent".into(),
                        pointer: String::new(),
                        waypoint_id: None,
                        message: "No trusted receipt".into(),
                    }],
                )
            })
    }
}

pub(super) fn fixture() -> (TempDir, MissionStore, Uuid, Vec<BaselineRequirementV4>) {
    let root = tempfile::tempdir().unwrap();
    let draft = MissionLifecycle::open(root.path())
        .create_draft("Search".into(), "Working search".into())
        .unwrap();
    let directory = root
        .path()
        .join(".workflow-tools/mission/missions")
        .join(draft.mission_id.to_string());
    fs::create_dir_all(directory.join("input")).unwrap();
    fs::write(
        directory.join("input/submitted-input.bin"),
        b"Working search",
    )
    .unwrap();
    let requirements = vec![BaselineRequirementV4 {
        requirement_id: "search".into(),
        source: IntentSourceSpan {
            source_sha256: intent_source_digest(b"Working search"),
            start: 0,
            end: 14,
            excerpt: "Working search".into(),
        },
        outcome: "Search returns matching records".into(),
        category: IntentOutcomeCategory::WorkingBehavior,
        evidence_policy: IntentEvidencePolicy::ExecutableBehavior { allow_mock: false },
    }];
    (
        root,
        MissionStore::open(directory.ancestors().nth(4).unwrap()),
        draft.mission_id,
        requirements,
    )
}

pub(super) fn provider(
    source: &[u8],
    baseline_sha256: String,
) -> (IntentReceiptReference, Provider) {
    let challenge = Uuid::new_v4();
    let reference = IntentReceiptReference {
        session_id: Uuid::new_v4().to_string(),
        challenge_id: challenge,
    };
    let receipt = IntentReceiptV4 {
        receipt_id: Uuid::new_v4().to_string(),
        challenge_id: challenge.to_string(),
        source_sha256: intent_source_digest(source),
        baseline_sha256,
        requester_event_id: Uuid::new_v4().to_string(),
        captured_at: "2026-11-08T18:00:00Z".into(),
        provider_version: "session-intent-confirmation/4".into(),
    };
    (reference, Provider(vec![receipt]))
}
