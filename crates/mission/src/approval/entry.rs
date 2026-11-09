use super::*;

pub struct EntryGuard<'a> {
    pub(super) _tx: ApprovalTransaction<'a>,
    pub grant: ApprovalGrant,
    pub(super) workspace: PathBuf,
    pub(super) evidence_admission: mission_api::EvidenceAdmissionV4,
}

impl EntryGuard<'_> {
    pub(crate) fn check_revision(&self, expected: u64) -> Result<(), ApprovalError> {
        if self._tx.review_plan()?.revision != expected {
            return Err(ApprovalError::new(
                "stale_plan_revision",
                "plan revision changed",
            ));
        }
        Ok(())
    }

    pub(crate) fn check_waypoint_entry(&self, waypoint_id: &str) -> Result<(), ApprovalError> {
        let runtime = self
            ._tx
            .control_state()
            .map_err(|e| ApprovalError::new(e.code(), e.to_string()))?;
        crate::WorkspaceLifecycle::check_prerequisites(
            &runtime,
            &self._tx.review_plan()?,
            waypoint_id,
        )
        .map_err(|e| ApprovalError::new(e.code(), e.to_string()))?;
        crate::provider_evidence::EvidenceGate {
            workspace: &self.workspace,
            admission: &self.evidence_admission,
        }
        .preflight(&self._tx, waypoint_id)
        .map_err(|e| ApprovalError::new(e.code(), e.to_string()))
    }

    pub fn record_progress(
        &self,
        waypoint_id: &str,
        state: WaypointState,
        expected: u64,
    ) -> Result<AcceptedMission, ApprovalError> {
        if state == WaypointState::InProgress {
            self.check_waypoint_entry(waypoint_id)?;
        }
        Ok(self._tx.record_progress(waypoint_id, state, expected)?)
    }
}
