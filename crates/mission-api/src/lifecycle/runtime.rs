//! Append-only lifecycle writes share the semantic publication lock.
use super::*;
use crate::ApprovalTransaction;
use std::collections::BTreeSet;

pub fn control_error(code: &'static str, message: impl Into<String>) -> LifecycleError {
    LifecycleError::ControlRejected {
        code,
        message: message.into(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionRecord {
    pub sequence: u64,
    pub waypoint_id: String,
    pub plan_digest: String,
    pub plan_revision: u64,
    pub result_ref: String,
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingReopen {
    pub operation_id: Uuid,
    pub completion_sequence: u64,
    pub waypoint_id: String,
    pub reason: String,
    pub ticket_id: Option<Uuid>,
    pub ticket_revision: Option<u64>,
    pub stale_waypoints: BTreeSet<String>,
    pub expected_plan_revision: u64,
    pub expected_control_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingHandoff {
    pub operation_id: Uuid,
    pub source: String,
    pub destination: String,
    pub reason: String,
    pub expected_control_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissionControlState {
    pub version: u32,
    pub sequence: u64,
    pub operations: BTreeSet<Uuid>,
    pub phase: LifecycleState,
    pub phase_reason: Option<String>,
    pub completions: Vec<CompletionRecord>,
    pub reopened: BTreeSet<String>,
    pub stale: BTreeSet<String>,
    pub pending_reopen: Option<PendingReopen>,
    pub pending_handoff: Option<PendingHandoff>,
    pub active_session: Option<String>,
    pub result_review: Option<String>,
}

impl Default for MissionControlState {
    fn default() -> Self {
        Self {
            version: 1,
            sequence: 0,
            operations: BTreeSet::new(),
            phase: LifecycleState::Planned,
            phase_reason: None,
            completions: vec![],
            reopened: BTreeSet::new(),
            stale: BTreeSet::new(),
            pending_reopen: None,
            pending_handoff: None,
            active_session: None,
            result_review: None,
        }
    }
}

impl MissionControlState {
    pub fn invalidate_other_plans(&mut self, plan_digest: &str) {
        for completion in &mut self.completions {
            if completion.current && completion.plan_digest != plan_digest {
                completion.current = false;
                self.stale.insert(completion.waypoint_id.clone());
                self.result_review = None;
            }
        }
    }

    pub fn check(&self, expected: u64) -> Result<(), LifecycleError> {
        if self.sequence != expected {
            return Err(control_error(
                "stale_control_sequence",
                format!("expected {expected}, current {}", self.sequence),
            ));
        }
        Ok(())
    }

    pub fn ensure_settled(&self) -> Result<(), LifecycleError> {
        if self.pending_reopen.is_some() || self.pending_handoff.is_some() {
            return Err(control_error(
                "cross_store_pending",
                "resume the recorded operation",
            ));
        }
        Ok(())
    }

    pub fn latest_completion(&self) -> Result<&CompletionRecord, LifecycleError> {
        self.completions
            .iter()
            .filter(|c| c.current)
            .max_by_key(|c| c.sequence)
            .ok_or_else(|| control_error("no_completed_waypoint", "no current completion"))
    }
}

impl ApprovalTransaction<'_> {
    pub fn control_state(&self) -> Result<MissionControlState, LifecycleError> {
        let names = self.list_records("controls").map_err(io_error)?;
        replay_controls(names, |name| {
            self.read_record(&format!("controls/{name}"))
                .map_err(io_error)?
                .ok_or_else(|| LifecycleError::JournalCorrupt(name.into()))
        })
    }

    /// Trusted facade persistence, like `write_record_new`. Not a transport
    /// operation: the domain facade must hold authority and verify foreign writes.
    pub fn write_control_state(
        &self,
        state: &mut MissionControlState,
        expected: u64,
    ) -> Result<(), LifecycleError> {
        self.control_state()?.check(expected)?;
        state.sequence = expected
            .checked_add(1)
            .ok_or_else(|| control_error("control_sequence_exhausted", "sequence exhausted"))?;
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        let name = format!("{:020}.json", state.sequence);
        let mut names = self.list_records("controls").map_err(io_error)?;
        names.push(name.clone());
        replay_controls(names, |entry| {
            if entry == name {
                return Ok(bytes.clone());
            }
            self.read_record(&format!("controls/{entry}"))
                .map_err(io_error)?
                .ok_or_else(|| LifecycleError::JournalCorrupt(entry.into()))
        })?;
        self.write_record_new(&format!("controls/{name}"), &bytes)
            .map_err(io_error)?;
        self.control_state()?;
        Ok(())
    }

    /// Version-1 controls are atomically replaced under the same publication lock.
    pub fn append_control(
        &self,
        workspace_root: &std::path::Path,
        event: ControlEvent,
    ) -> Result<(), LifecycleError> {
        use std::{fs, io::Write};
        let mut journal = LifecycleJournal::read(workspace_root, self.mission_id())?;
        let last = journal.projection()?.last_seq;
        if event.prior_seq != last {
            return Err(control_error(
                "stale_control_sequence",
                "journal sequence changed",
            ));
        }
        journal.events.push(event);
        journal.projection()?;
        let path = LifecycleJournal::path(workspace_root, self.mission_id());
        let parent = path
            .parent()
            .ok_or_else(|| io_error("journal has no parent"))?;
        let scratch = parent.join(format!(".control-{}", Uuid::new_v4()));
        let result = (|| -> Result<(), LifecycleError> {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&scratch)
                .map_err(io_error)?;
            file.write_all(journal.to_json()?.as_bytes())
                .map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            fs::rename(&scratch, &path).map_err(io_error)?;
            fs::File::open(parent)
                .and_then(|f| f.sync_all())
                .map_err(io_error)?;
            Ok(())
        })();
        if scratch.exists() {
            fs::remove_file(scratch).map_err(io_error)?;
        }
        result
    }
}

fn replay_controls(
    names: Vec<String>,
    read: impl Fn(&str) -> Result<Vec<u8>, LifecycleError>,
) -> Result<MissionControlState, LifecycleError> {
    let mut state = MissionControlState::default();
    for (index, name) in names.iter().enumerate() {
        let seq = index as u64 + 1;
        if name != &format!("{seq:020}.json") {
            return Err(LifecycleError::JournalCorrupt(format!(
                "invalid control entry {name}"
            )));
        }
        let bytes = read(name)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        if value["version"].as_u64() != Some(1) {
            return Err(LifecycleError::JournalUnsupportedVersion {
                found: value["version"].to_string(),
                supported: 1,
            });
        }
        let next: MissionControlState = serde_json::from_value(value)
            .map_err(|e| LifecycleError::JournalCorrupt(e.to_string()))?;
        if next.sequence != seq
            || !next.operations.is_superset(&state.operations)
            || !valid_phase_change(&state, &next)
            || !matches!(
                next.phase,
                LifecycleState::Planned
                    | LifecycleState::Active
                    | LifecycleState::Blocked
                    | LifecycleState::Completed
            )
            || (next.phase == LifecycleState::Completed
                && (next.result_review.is_none()
                    || next
                        .phase_reason
                        .as_ref()
                        .is_none_or(|s| s.trim().is_empty())
                    || next.completions.is_empty()))
            || next.completions.len() < state.completions.len()
            || state
                .completions
                .iter()
                .zip(&next.completions)
                .any(|(old, new)| {
                    old.sequence != new.sequence
                        || old.waypoint_id != new.waypoint_id
                        || old.plan_digest != new.plan_digest
                        || old.plan_revision != new.plan_revision
                        || old.result_ref != new.result_ref
                        || (!old.current && new.current)
                })
            || next.completions.iter().enumerate().any(|(index, c)| {
                c.sequence != index as u64 + 1
                    || c.result_ref.trim().is_empty()
                    || c.waypoint_id.trim().is_empty()
                    || c.plan_digest.trim().is_empty()
            })
        {
            return Err(LifecycleError::JournalCorrupt(format!(
                "invalid control history at {seq}"
            )));
        }
        state = next;
    }
    Ok(state)
}

fn valid_phase_change(old: &MissionControlState, next: &MissionControlState) -> bool {
    if let Some(pending) = &next.pending_reopen {
        let valid_completion = old.completions.iter().any(|completion| {
            completion.current
                && completion.sequence == pending.completion_sequence
                && completion.waypoint_id == pending.waypoint_id
                && completion.plan_revision <= pending.expected_plan_revision
        });
        if !valid_completion
            || pending.operation_id.is_nil()
            || pending.reason.trim().is_empty()
            || !pending.stale_waypoints.contains(&pending.waypoint_id)
            || pending.ticket_id.is_some() != pending.ticket_revision.is_some()
            || old.operations.contains(&pending.operation_id)
            || old
                .pending_reopen
                .as_ref()
                .is_some_and(|prior| prior != pending)
        {
            return false;
        }
    }
    let finalized_reopen = old.pending_reopen.as_ref().is_some_and(|pending| {
        next.pending_reopen.is_none()
            && next.operations.contains(&pending.operation_id)
            && next.reopened.contains(&pending.waypoint_id)
            && pending.stale_waypoints.is_subset(&next.stale)
            && !next.completions.iter().any(|completion| {
                completion.current && pending.stale_waypoints.contains(&completion.waypoint_id)
            })
    });
    if old.pending_reopen.is_some() && next.pending_reopen.is_none() && !finalized_reopen {
        return false;
    }
    if old.phase == next.phase {
        return true;
    }
    match (old.phase, next.phase) {
        (_, LifecycleState::Planned) => finalized_reopen,
        (LifecycleState::Completed | LifecycleState::Cancelled, _) => false,
        (LifecycleState::Planned | LifecycleState::Blocked, LifecycleState::Active)
        | (LifecycleState::Active, LifecycleState::Blocked)
        | (_, LifecycleState::Completed) => true,
        _ => false,
    }
}

fn io_error(error: impl std::fmt::Display) -> LifecycleError {
    LifecycleError::JournalIo(error.to_string())
}

/// Caller holds the publication lock. Planning cannot replace the route during
/// a partially committed cross-store operation.
pub(crate) fn ensure_control_settled(
    mission_dir: &std::path::Path,
) -> Result<(), crate::MissionError> {
    use std::fs;
    let read = || -> Result<(), LifecycleError> {
        let dir = mission_dir.join("approvals/controls");
        for path in [mission_dir.join("approvals"), dir.clone()] {
            match fs::symlink_metadata(&path) {
                Ok(meta) if meta.file_type().is_symlink() || !meta.is_dir() => {
                    return Err(LifecycleError::JournalCorrupt(format!(
                        "unsafe control directory {}",
                        path.display()
                    )));
                }
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(e) => return Err(io_error(e)),
            }
        }
        let mut names = fs::read_dir(&dir)
            .map_err(io_error)?
            .map(|e| {
                e.map(|e| e.file_name().to_string_lossy().into_owned())
                    .map_err(io_error)
            })
            .collect::<Result<Vec<_>, _>>()?;
        names.retain(|n| !n.starts_with('.'));
        names.sort();
        replay_controls(names, |name| {
            let path = dir.join(name);
            let meta = fs::symlink_metadata(&path).map_err(io_error)?;
            if meta.file_type().is_symlink() || !meta.is_file() {
                return Err(LifecycleError::JournalCorrupt(format!(
                    "unsafe control record {name}"
                )));
            }
            fs::read(path).map_err(io_error)
        })?
        .ensure_settled()
    };
    read().map_err(|e| {
        crate::MissionError::Validation(
            1,
            vec![crate::Diagnostic {
                code: e.code(),
                source: "lifecycle".into(),
                pointer: String::new(),
                waypoint_id: None,
                message: e.to_string(),
            }],
        )
    })
}
