use super::*;

fn classify(error: SessionError) -> Result<IntakeError, SessionError> {
    Ok(match error {
        SessionError::IntakeSessionNotFound { session_id } => {
            IntakeError::SessionNotFound { session_id }
        }
        SessionError::IntakeMissing {
            session_id,
            request_id,
        } => IntakeError::IntakeMissing {
            session_id,
            request_id,
        },
        SessionError::WorkspaceFinished { session_id } => {
            IntakeError::SessionFinished { session_id }
        }
        SessionError::IntakeIdempotencyConflict {
            request_id,
            existing_digest,
            submitted_digest,
            ..
        } => IntakeError::IdempotencyConflict {
            request_id,
            existing_digest,
            submitted_digest,
        },
        SessionError::InvalidIntakeRequestId(value) => IntakeError::InvalidRequest {
            reason: format!("request_id `{value}` is not a safe identifier"),
        },
        SessionError::InvalidSessionId(value) => IntakeError::InvalidRequest {
            reason: format!("session id `{value}` is not a Session UUID"),
        },
        SessionError::RuntimeMutationConflict { session_id } => IntakeError::Busy {
            reason: format!("another operation holds the Session lock for {session_id}"),
        },
        SessionError::IntakeLinkageConflict { reason, .. } => {
            IntakeError::LinkageConflict { reason }
        }
        SessionError::IntakeCorrupt { path, reason } => IntakeError::IncompatibleStore {
            reason: format!("{}: {reason}", path.display()),
        },
        SessionError::InvalidStorePath(path) => IntakeError::IncompatibleStore {
            reason: format!("unsafe or symlinked Session path {}", path.display()),
        },
        other => return Err(other),
    })
}

pub(super) fn halt(error: SessionError) -> Halt {
    match classify(error) {
        Ok(classified) => Halt::Final(classified),
        Err(other) => Halt::Failed(other.to_string()),
    }
}

pub(super) fn to_final(error: SessionError) -> IntakeError {
    classify(error).unwrap_or_else(|other| IntakeError::Storage {
        reason: other.to_string(),
    })
}

pub(super) fn pending(request: &ManagedIntakeRequest<'_>, cause: String) -> IntakeError {
    match persisted_state(
        request.session_store,
        request.workspace_root,
        request.session_id,
        request.request_id,
    ) {
        Ok(Some((mission_id, persisted))) => IntakeError::Pending(PendingReceipt {
            session_id: request.session_id.to_string(),
            request_id: request.request_id.to_string(),
            mission_id,
            persisted,
            cause,
        }),
        Ok(None) => IntakeError::Storage { reason: cause },
        Err(error) => IntakeError::Storage {
            reason: format!("{cause}; persisted state could not be inspected: {error}"),
        },
    }
}

pub(super) fn unavailable(reason: impl Into<String>) -> IntakeError {
    IntakeError::WorkspaceUnavailable {
        reason: reason.into(),
    }
}

pub(super) fn incompatible(reason: impl Into<String>) -> IntakeError {
    IntakeError::IncompatibleStore {
        reason: reason.into(),
    }
}
