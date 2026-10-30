use super::*;

pub(crate) struct Fixture {
    _dir: tempfile::TempDir,
    pub(crate) session_root: PathBuf,
    pub(crate) workspace: PathBuf,
}

pub(crate) fn fixture() -> Fixture {
    let dir = tempfile::TempDir::new().unwrap();
    let session_root = dir.path().join("session-store");
    let workspace = dir.path().join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    SessionStoreConfig::new(&session_root)
        .init_runtime_context(session_api::SessionRuntimeInitRequest {
            session_id: Some(SESSION.to_string()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();
    Fixture {
        _dir: dir,
        session_root,
        workspace,
    }
}

impl Fixture {
    pub(crate) fn session(&self) -> SessionStoreConfig {
        SessionStoreConfig::new(&self.session_root)
    }

    pub(crate) fn submit(
        &self,
        request_id: &str,
        input: &[u8],
    ) -> Result<ManagedIntake, IntakeError> {
        let session = self.session();
        submit_and_create(&ManagedIntakeRequest {
            session_store: &session,
            workspace_root: &self.workspace,
            session_id: SESSION,
            request_id,
            input,
        })
    }

    pub(crate) fn submit_with_fault(
        &self,
        request_id: &str,
        input: &[u8],
        fault: IntakeStep,
    ) -> Result<ManagedIntake, IntakeError> {
        let session = self.session();
        submit_and_create_with_fault(
            &ManagedIntakeRequest {
                session_store: &session,
                workspace_root: &self.workspace,
                session_id: SESSION,
                request_id,
                input,
            },
            Some(fault),
        )
    }

    pub(crate) fn find(&self, request_id: &str) -> Result<ManagedIntake, IntakeError> {
        find(&self.session(), &self.workspace, SESSION, request_id)
    }

    pub(crate) fn missions_root(&self) -> PathBuf {
        self.workspace.join(".workflow-tools/mission/missions")
    }

    pub(crate) fn mission_count(&self) -> usize {
        match std::fs::read_dir(self.missions_root()) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| Uuid::parse_str(&entry.file_name().to_string_lossy()).is_ok())
                .count(),
            Err(_) => 0,
        }
    }
}
