use super::*;

#[derive(Subcommand)]
pub(super) enum CommandOperation {
    /// Manage requester-confirmed intent; action is a strict JSON IntentOperation.
    Intent {
        mission_id: Uuid,
        action: String,
    },
    /// Record one cold or formal review from strict JSON ReviewInput.
    RecordReview {
        mission_id: Uuid,
        request_id: String,
        input: String,
    },
    /// Activate a Ticket-backed waypoint through Mission approval and prerequisite guards.
    ActivateWaypoint {
        mission_id: Uuid,
        waypoint_id: String,
        #[arg(long)]
        request_id: Uuid,
        #[arg(long)]
        expected_revision: u64,
        /// Ticket history metadata, not approval authority.
        #[arg(long)]
        author: Option<String>,
    },
    CreateMission {
        title: String,
        objective: String,
        waypoints: String,
    },
    CreateDraft {
        title: String,
        objective: String,
    },
    SaveDraft {
        mission_id: Uuid,
        draft: String,
        expected_current_revision: u64,
    },
    PublishDraft {
        mission_id: Uuid,
        draft: String,
        expected_current_revision: u64,
    },
    GetMissionSource {
        mission_id: Uuid,
    },
    RenderMission {
        mission_id: Uuid,
    },
    EditMissionObjective {
        mission_id: Uuid,
        objective: String,
        expected_current_revision: u64,
    },
    AddWaypoint {
        mission_id: Uuid,
        waypoint_id: String,
        title: String,
        #[command(flatten)]
        content: WaypointContent,
        expected_current_revision: u64,
    },
    EditWaypoint {
        mission_id: Uuid,
        waypoint_id: String,
        title: String,
        #[command(flatten)]
        content: WaypointContent,
        expected_current_revision: u64,
    },
    OrderWaypoints {
        mission_id: Uuid,
        #[arg(value_delimiter = ',', required = true, num_args = 1)]
        waypoint_ids: Vec<String>,
        expected_current_revision: u64,
        #[arg(long)]
        waypoint_dependencies: Option<String>,
    },
    RecordProgress {
        mission_id: Uuid,
        waypoint_id: String,
        state: String,
        expected_current_revision: u64,
    },
    RecordValidationResult {
        mission_id: Uuid,
        waypoint_id: String,
        outcome: String,
        operator_command: String,
        evidence_pointer: String,
        expected_current_revision: u64,
    },
    ValidateMission {
        mission_id: Uuid,
    },
    PublishMission {
        mission_id: Uuid,
        expected_current_revision: u64,
    },
    #[command(alias = "get-mission")]
    Get {
        mission_id: Uuid,
    },
    ValidatePreview {
        manifest: std::path::PathBuf,
    },
    Import {
        mission_id: Uuid,
        manifest: std::path::PathBuf,
        expected_current_revision: u64,
    },
    RenderPreview {
        mission_id: Uuid,
    },
    Publish {
        mission_id: Uuid,
        manifest: std::path::PathBuf,
        expected_current_revision: u64,
    },
    CheckGenerated {
        mission_id: Uuid,
    },
    /// Validate and read the complete pre-reaction output inventory.
    GetOutputContract {
        mission_id: Uuid,
    },
    GetApprovalEvent {
        mission_id: Uuid,
        revision: u64,
    },
    /// Record one reaction JSON object; signature is a digest link, not authentication.
    RecordApprovalEvent {
        mission_id: Uuid,
        json: String,
    },
    /// Publish with a JSON object containing merged_request and independent continuations.
    PublishWithInputs {
        mission_id: Uuid,
        manifest: std::path::PathBuf,
        expected_current_revision: u64,
        inputs: String,
    },
    MigrateDossiers {
        #[arg(long)]
        dossier: Option<std::path::PathBuf>,
        #[arg(long, default_value_t = false)]
        dry_run: bool,
    },
    MigrateTypedStores {
        #[arg(long = "root")]
        roots: Vec<std::path::PathBuf>,
    },
    RepairPreview {
        #[arg(long, required = true)]
        records: String,
    },
    RepairApply {
        #[arg(long, required = true)]
        records: String,
        #[arg(long, required = true)]
        proposal_id: String,
    },
    RepairJournal {
        #[arg(long)]
        journal_id: Uuid,
    },
    RepairResume {
        #[arg(long)]
        journal_id: Uuid,
    },
    RepairRollback {
        #[arg(long)]
        journal_id: Uuid,
    },
}

#[derive(Args)]
pub(super) struct WaypointContent {
    #[arg(long, value_delimiter = ',')]
    depends_on: Vec<String>,
    #[arg(long)]
    prompt: String,
    #[arg(long = "validation-command", required = true)]
    validation_commands: Vec<String>,
    #[arg(long)]
    validation_cwd: Option<String>,
    #[arg(long)]
    checkpoint: String,
    #[arg(long = "non-goal")]
    non_goals: Vec<String>,
}

impl WaypointContent {
    pub(super) fn waypoint(self, waypoint_id: String, title: String) -> DirectWaypoint {
        DirectWaypoint {
            waypoint_id,
            title,
            depends_on: self.depends_on,
            prompt: self.prompt,
            validation: self
                .validation_commands
                .into_iter()
                .map(|command| ValidationGate {
                    command,
                    cwd: self.validation_cwd.clone(),
                })
                .collect(),
            commit_checkpoint: CommitCheckpoint {
                message: self.checkpoint,
            },
            non_goals: self.non_goals,
        }
    }
}
