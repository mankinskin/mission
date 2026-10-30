use super::*;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};
// ---------------------------------------------------------------------------
// Version-1 control journal
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ControlTarget {
    Mission,
    Waypoint { waypoint_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlEvent {
    pub seq: u64,
    pub op_id: Uuid,
    pub target: ControlTarget,
    /// Sequence of the preceding event (0 for the first event).
    pub prior_seq: u64,
    pub event: ControlKind,
    /// Nonterminal state before the event (for resume: the restored state).
    pub prior_state: LifecycleState,
    /// Required for completion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Replayed controls for every target of one Mission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ControlProjection {
    pub last_seq: u64,
    targets: BTreeMap<ControlTarget, TargetControl>,
    op_ids: BTreeSet<Uuid>,
}

impl ControlProjection {
    pub fn target(&self, target: &ControlTarget) -> TargetControl {
        self.targets.get(target).cloned().unwrap_or_default()
    }

    fn apply(&mut self, event: &ControlEvent) -> Result<(), LifecycleError> {
        let invalid = |reason: String| LifecycleError::InvalidControlHistory {
            seq: event.seq,
            reason,
        };
        if event.seq == 0 || event.seq <= self.last_seq {
            return Err(invalid(format!("sequence must exceed {}", self.last_seq)));
        }
        if event.prior_seq != self.last_seq {
            return Err(invalid(format!(
                "prior sequence {} does not match preceding sequence {}",
                event.prior_seq, self.last_seq
            )));
        }
        if self.op_ids.contains(&event.op_id) {
            return Err(invalid(format!("duplicate operation id {}", event.op_id)));
        }
        self.targets
            .entry(event.target.clone())
            .or_default()
            .apply_control(
                event.event,
                event.prior_state,
                event.evidence.as_deref(),
                event.seq,
            )
            .map_err(|error| match error {
                memory_kernel::lifecycle::LifecycleError::IllegalTransition(reason) => {
                    invalid(reason)
                }
                other => invalid(other.to_string()),
            })?;
        self.op_ids.insert(event.op_id);
        self.last_seq = event.seq;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LifecycleJournal {
    pub version: u32,
    pub events: Vec<ControlEvent>,
}

impl Default for LifecycleJournal {
    /// The empty journal; equivalent to a legacy Mission without a journal.
    fn default() -> Self {
        Self {
            version: LIFECYCLE_JOURNAL_VERSION,
            events: Vec::new(),
        }
    }
}

impl LifecycleJournal {
    pub fn path(workspace_root: &Path, mission_id: Uuid) -> PathBuf {
        workspace_root
            .join(".workflow-tools")
            .join("mission")
            .join("missions")
            .join(mission_id.to_string())
            .join(LIFECYCLE_JOURNAL_FILE)
    }

    pub fn to_json(&self) -> Result<String, LifecycleError> {
        serde_json::to_string_pretty(self)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))
    }

    /// Parses and validates a journal; unsupported versions and corrupt
    /// content are explicit errors.
    pub fn from_json(text: &str) -> Result<Self, LifecycleError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        let version = value
            .get("version")
            .ok_or_else(|| LifecycleError::JournalCorrupt("missing version".into()))?;
        if version.as_u64() != Some(u64::from(LIFECYCLE_JOURNAL_VERSION)) {
            return Err(LifecycleError::JournalUnsupportedVersion {
                found: version.to_string(),
                supported: LIFECYCLE_JOURNAL_VERSION,
            });
        }
        let journal: Self = serde_json::from_value(value)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        journal.projection()?;
        Ok(journal)
    }

    /// Reads the journal of a Mission. A missing file is a legacy Mission with
    /// no controls (not an approval); every other failure is an error.
    pub fn read(workspace_root: &Path, mission_id: Uuid) -> Result<Self, LifecycleError> {
        match fs::read_to_string(Self::path(workspace_root, mission_id)) {
            Ok(text) => Self::from_json(&text),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(LifecycleError::JournalIo(error.to_string())),
        }
    }

    /// Replays and validates the full history.
    pub fn projection(&self) -> Result<ControlProjection, LifecycleError> {
        let mut projection = ControlProjection::default();
        for event in &self.events {
            projection.apply(event)?;
        }
        Ok(projection)
    }

    /// Rejects waypoint targets that are not members of the Mission.
    pub fn validate_waypoint_targets(
        &self,
        known: &BTreeSet<String>,
    ) -> Result<(), LifecycleError> {
        for event in &self.events {
            if let ControlTarget::Waypoint { waypoint_id } = &event.target
                && !known.contains(waypoint_id)
            {
                return Err(LifecycleError::InvalidControlHistory {
                    seq: event.seq,
                    reason: format!(
                        "target waypoint '{waypoint_id}' is not a control target of this Mission"
                    ),
                });
            }
        }
        Ok(())
    }

    /// Builds the next legal event for `target` without persisting it. Fails
    /// with `illegal_transition` for edges the control contract forbids.
    pub fn next_event(
        &self,
        target: ControlTarget,
        kind: ControlKind,
        current_state: LifecycleState,
        op_id: Uuid,
        evidence: Option<String>,
    ) -> Result<ControlEvent, LifecycleError> {
        let mut projection = self.projection()?;
        let control = projection.target(&target);
        let prior_state = match kind {
            ControlKind::Resume => control.paused_from.unwrap_or(current_state),
            _ if current_state == LifecycleState::Paused => {
                control.paused_from.unwrap_or(current_state)
            }
            _ => current_state,
        };
        let event = ControlEvent {
            seq: projection.last_seq.checked_add(1).ok_or_else(|| {
                LifecycleError::IllegalTransition("control sequence exhausted".into())
            })?,
            op_id,
            target,
            prior_seq: projection.last_seq,
            event: kind,
            prior_state,
            evidence,
        };
        projection.apply(&event).map_err(|error| match error {
            LifecycleError::InvalidControlHistory { reason, .. } => {
                LifecycleError::IllegalTransition(reason)
            }
            other => other,
        })?;
        Ok(event)
    }
}
