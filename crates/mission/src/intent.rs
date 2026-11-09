//! Requester-confirmed intent authority, separate from the editable Mission plan.

use crate::*;
use serde::{Deserialize, Serialize};
use session_api::{
    IntentConfirmationChallenge, IntentConfirmationChallengeRequest, SessionStoreConfig,
};

#[derive(Clone, Debug, Deserialize)]
#[cfg_attr(any(feature = "cli", feature = "mcp"), derive(schemars::JsonSchema))]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum IntentOperation {
    PrepareBaseline {
        expected_revision: u64,
        requirements: Vec<BaselineRequirementV4>,
        reference: IntentReceiptReference,
    },
    ConfirmBaseline {
        expected_revision: u64,
        requirements: Vec<BaselineRequirementV4>,
        reference: IntentReceiptReference,
    },
    PrepareAmendment {
        expected_revision: u64,
        amendment_id: String,
        prior_chain_sha256: String,
        source: String,
        changes: Vec<IntentChangeV4>,
        reference: IntentReceiptReference,
    },
    AppendAmendment {
        expected_revision: u64,
        amendment_id: String,
        prior_chain_sha256: String,
        source: String,
        changes: Vec<IntentChangeV4>,
        reference: IntentReceiptReference,
    },
    Get {},
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IntentOutput {
    Challenge {
        challenge: IntentConfirmationChallenge,
        prompt: String,
    },
    Baseline {
        baseline: IntentBaselineV4,
    },
    Amendment {
        amendment: IntentAmendmentV4,
    },
    Effective {
        intent: Option<EffectiveIntentV4>,
    },
}

fn error(code: &'static str, message: impl ToString) -> MissionError {
    MissionError::Validation(
        1,
        vec![Diagnostic {
            code,
            source: "intent".into(),
            pointer: String::new(),
            waypoint_id: None,
            message: message.to_string(),
        }],
    )
}

fn session_error(error_value: session_api::SessionError) -> MissionError {
    error(
        error_value
            .requester_code()
            .unwrap_or("intent-provider-unavailable"),
        error_value,
    )
}

pub struct SessionIntentProvider {
    store: SessionStoreConfig,
    workspace_root: PathBuf,
}

impl SessionIntentProvider {
    pub fn new(workspace_root: &Path) -> Result<Self, MissionError> {
        let (root, _) = resolve_workspace(Some(workspace_root), None, workspace_root)
            .map_err(|e| error("intent-workspace-invalid", e))?;
        let (session_root, _) =
            resolve_workspace(Some(&root.join(".workflow-tools/session")), None, &root)
                .map_err(|e| error("intent-workspace-invalid", e))?;
        Ok(Self {
            store: SessionStoreConfig::new(session_root),
            workspace_root: root,
        })
    }

    fn prepare(
        &self,
        mission_id: Uuid,
        reference: IntentReceiptReference,
        source_sha256: String,
        baseline_sha256: String,
    ) -> Result<IntentOutput, MissionError> {
        let challenge = self
            .store
            .register_intent_confirmation_challenge(&IntentConfirmationChallengeRequest {
                session_id: reference.session_id.clone(),
                challenge_id: reference.challenge_id.to_string(),
                mission_id,
                mission_root: self.workspace_root.clone(),
                source_sha256,
                baseline_sha256,
            })
            .map_err(session_error)?;
        let prompt = self.store.render_intent_confirmation_prompt(&challenge);
        Ok(IntentOutput::Challenge { challenge, prompt })
    }
}

impl IntentReceiptProvider for SessionIntentProvider {
    fn confirmed_receipt(
        &self,
        mission_id: Uuid,
        reference: &IntentReceiptReference,
    ) -> Result<IntentReceiptV4, MissionError> {
        let challenge = self
            .store
            .read_intent_confirmation_challenge(
                &reference.session_id,
                &reference.challenge_id.to_string(),
            )
            .map_err(session_error)?;
        if challenge.request.mission_id != mission_id
            || challenge.request.mission_root != self.workspace_root
        {
            return Err(error(
                "intent-confirmation-mismatch",
                "Confirmation belongs to another Mission or workspace.",
            ));
        }
        let receipt = self
            .store
            .read_intent_confirmation(&reference.session_id, &reference.challenge_id.to_string())
            .map_err(session_error)?;
        Ok(IntentReceiptV4 {
            receipt_id: receipt.receipt_id,
            challenge_id: receipt.challenge_id,
            source_sha256: receipt.source_sha256,
            baseline_sha256: receipt.baseline_sha256,
            requester_event_id: receipt.requester_event_id,
            captured_at: receipt.captured_at,
            provider_version: receipt.provider_version,
        })
    }
}

pub fn execute_intent(
    workspace_root: &Path,
    mission_id: Uuid,
    operation: IntentOperation,
) -> Result<(u64, IntentOutput), MissionError> {
    let provider = SessionIntentProvider::new(workspace_root)?;
    let store = MissionStore::open(workspace_root);
    let result = match operation {
        IntentOperation::PrepareBaseline {
            expected_revision,
            requirements,
            reference,
        } => {
            let (source, baseline) =
                store.proposed_intent_baseline(mission_id, expected_revision, &requirements)?;
            provider.prepare(mission_id, reference, source, baseline)?
        }
        IntentOperation::ConfirmBaseline {
            expected_revision,
            requirements,
            reference,
        } => IntentOutput::Baseline {
            baseline: store.confirm_intent_baseline(
                mission_id,
                expected_revision,
                requirements,
                reference,
                &provider,
            )?,
        },
        IntentOperation::PrepareAmendment {
            expected_revision,
            amendment_id,
            prior_chain_sha256,
            source,
            changes,
            reference,
        } => {
            let request = IntentAmendmentRequest {
                expected_revision,
                amendment_id,
                prior_chain_sha256,
                source,
                changes,
                reference: reference.clone(),
            };
            let (source_sha, amendment_sha) =
                store.proposed_intent_amendment(mission_id, &request, &provider)?;
            provider.prepare(mission_id, reference, source_sha, amendment_sha)?
        }
        IntentOperation::AppendAmendment {
            expected_revision,
            amendment_id,
            prior_chain_sha256,
            source,
            changes,
            reference,
        } => IntentOutput::Amendment {
            amendment: store.append_intent_amendment(
                mission_id,
                IntentAmendmentRequest {
                    expected_revision,
                    amendment_id,
                    prior_chain_sha256,
                    source,
                    changes,
                    reference,
                },
                &provider,
            )?,
        },
        IntentOperation::Get {} => IntentOutput::Effective {
            intent: store.effective_intent(mission_id, &provider)?,
        },
    };
    let lifecycle = MissionLifecycle::open(workspace_root);
    let revision = lifecycle.get_mission_source(mission_id)?.revision;
    Ok((revision, result))
}
