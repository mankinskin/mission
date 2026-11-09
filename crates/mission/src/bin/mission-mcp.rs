use std::path::PathBuf;

use mission::named::{self, Input};
use mission::{
    CommitCheckpoint, DirectWaypoint, MISSION_WORKSPACE_ENV, Operation, OperatorResult,
    RepairRequest, TypedDraft, ValidationGate, WaypointState, execute_mcp, resolve_workspace,
};
use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content},
    schemars::JsonSchema,
    tool, tool_handler, tool_router,
};
use serde::Deserialize;
use transport_harness::HarnessError;
use transport_harness::cli::clap::{Parser, error::ErrorKind};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "mission-mcp")]
struct Launch {
    #[arg(long)]
    workspace: Option<PathBuf>,
}

#[derive(Clone)]
struct MissionServer {
    workspace: PathBuf,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct MissionOperationInput {
    /// Strict requester-intent action, required for operation=intent.
    #[serde(default)]
    intent_action: Option<mission::intent::IntentOperation>,
    /// Exact operation name from the tool description; underscores and hyphens are not interchangeable. Missing operation-specific fields return invalid_params.
    operation: String,
    /// Stored Mission UUID, required except create_draft, create_mission, validate-preview, migrate-dossiers and repair operations with their own request/journal identity.
    #[serde(default)]
    mission_id: Option<String>,
    /// Manifest file for validate-preview/import/publish/publish-with-inputs. Not used for typed draft operations, which accept inline draft.
    #[serde(default)]
    manifest_path: Option<PathBuf>,
    /// Exact current stored revision for revision-checked mutations; 0 only for a verified absent Mission in bundle import/publish. Omission does not disable a required CAS check.
    #[serde(default)]
    expected_current_revision: Option<u64>,
    /// Optional selected classic dossier for migrate-dossiers; omission scans the configured workspace's transcript dossiers. Migration is a write unless dry_run=true.
    #[serde(default)]
    dossier_path: Option<PathBuf>,
    /// migrate-dossiers preview flag only; defaults false. This is not a general mutation preview or repair dry-run flag.
    #[serde(default)]
    dry_run: bool,
    /// Initial title required for create_draft/create_mission.
    #[serde(default)]
    title: Option<String>,
    /// Objective required for create_draft/create_mission/edit_mission_objective.
    #[serde(default)]
    objective: Option<String>,
    /// Complete inline typed Mission source for save_draft/publish_draft; not a client path or manifest.
    #[serde(default)]
    draft: Option<TypedDraft>,
    /// Complete legacy waypoint collection required for create_mission; use draft for typed source operations.
    #[serde(default)]
    waypoints: Option<Vec<McpWaypoint>>,
    /// Stable local waypoint ID for add_waypoint/edit_waypoint/record_progress/record_validation_result.
    #[serde(default)]
    waypoint_id: Option<String>,
    /// Waypoint title required for add_waypoint/edit_waypoint.
    #[serde(default)]
    waypoint_title: Option<String>,
    /// Local waypoint prerequisite IDs for add_waypoint/edit_waypoint; defaults empty, not cross-Mission URNs.
    #[serde(default)]
    depends_on: Vec<String>,
    /// Complete waypoint prompt required for add_waypoint/edit_waypoint.
    #[serde(default)]
    prompt: Option<String>,
    /// Validation command definitions required for add_waypoint/edit_waypoint; recording does not execute commands.
    #[serde(default)]
    validation: Option<Vec<McpValidationGate>>,
    /// Commit message definition required for add_waypoint/edit_waypoint; does not create a commit.
    #[serde(default)]
    commit_checkpoint: Option<McpCheckpoint>,
    /// Explicit waypoint exclusions for add_waypoint/edit_waypoint; defaults empty.
    #[serde(default)]
    non_goals: Vec<String>,
    /// Complete ordered stable local waypoint IDs for order_waypoints; omission supplies an empty list, not the current order.
    #[serde(default)]
    execution_order: Vec<String>,
    /// Local waypoint-to-prerequisite mapping for order_waypoints; defaults empty, not an unchanged-map sentinel.
    #[serde(default)]
    waypoint_dependencies: std::collections::BTreeMap<String, Vec<String>>,
    /// record_progress state: pending, in-progress, completed or blocked. Progress records are not native lifecycle completion authority.
    #[serde(default)]
    state: Option<String>,
    /// Result outcome text required for record_validation_result.
    #[serde(default)]
    outcome: Option<String>,
    /// Command evidence required for record_validation_result; this tool records it, never runs it.
    #[serde(default)]
    operator_command: Option<String>,
    /// Evidence location/identity required for record_validation_result.
    #[serde(default)]
    evidence_pointer: Option<String>,
    /// Complete typed journaled repair request for repair_preview/repair_apply.
    #[serde(default)]
    repair_request: Option<RepairRequest>,
    /// Proposal identity returned by matching repair_preview, required for repair_apply.
    #[serde(default)]
    proposal_id: Option<String>,
    /// Existing repair journal UUID for repair_journal/repair_resume/repair_rollback.
    #[serde(default)]
    journal_id: Option<String>,
    /// Complete merged request and create-new continuation inputs required for publish-with-inputs.
    #[serde(default)]
    output_inputs: Option<mission::MissionOutputInputs>,
    /// Strict approval-event JSON required for record-approval-event; captured reaction metadata is not native host approval or execution authority.
    #[serde(default)]
    approval_event_json: Option<String>,
    /// Accepted revision whose reaction is read by get-approval-event; required for that operation, not a mutation CAS field.
    #[serde(default)]
    revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpWaypoint {
    /// Stable Mission-local waypoint identity, not a filename or Mission UUID.
    waypoint_id: String,
    /// Human-readable waypoint title.
    title: String,
    /// Local prerequisite waypoint IDs; defaults empty.
    #[serde(default)]
    depends_on: Vec<String>,
    /// Complete execution prompt.
    prompt: String,
    /// Stored validation definitions; no command is executed by creation/editing.
    validation: Vec<McpValidationGate>,
    /// Stored commit-message definition; no commit is made by this operation.
    commit_checkpoint: McpCheckpoint,
    /// Explicit scope exclusions; defaults empty.
    #[serde(default)]
    non_goals: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpValidationGate {
    /// Validation command text to store, not execute.
    command: String,
    /// Optional command working directory retained for later validation.
    #[serde(default)]
    cwd: Option<String>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct McpCheckpoint {
    /// Intended checkpoint commit message; recording it does not commit.
    message: String,
}

impl From<McpWaypoint> for DirectWaypoint {
    fn from(input: McpWaypoint) -> Self {
        Self {
            waypoint_id: input.waypoint_id,
            title: input.title,
            depends_on: input.depends_on,
            prompt: input.prompt,
            validation: input
                .validation
                .into_iter()
                .map(|gate| ValidationGate {
                    command: gate.command,
                    cwd: gate.cwd,
                })
                .collect(),
            commit_checkpoint: CommitCheckpoint {
                message: input.commit_checkpoint.message,
            },
            non_goals: input.non_goals,
        }
    }
}

impl MissionServer {
    fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            tool_router: Self::tool_router(),
        }
    }

    fn named<T>(&self, name: &str, input: Input<T>) -> Result<CallToolResult, McpError> {
        let envelope = named::call(name, input.0);
        let failed = envelope.code != "ok";
        let text = serde_json::to_string(&envelope)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(if failed {
            CallToolResult::error(vec![Content::text(text)])
        } else {
            CallToolResult::success(vec![Content::text(text)])
        })
    }
}

#[tool_router]
impl MissionServer {
    #[tool(
        name = "mission_plan_record_review",
        description = "capability=mission.native-planning version=1. Record a revision/digest-bound cold_walkthrough or distinct formal review; formal references a passing current cold review. Replay identical request/evidence only. Reviewer evidence never grants requester approval. Normally omit session_root; an explicit value is the exact separate Session store directory, not a repo root. Returns JSON code/revision/result/details; code!=ok is an MCP tool error."
    )]
    async fn plan_record_review(
        &self,
        Parameters(i): Parameters<Input<named::RecordReviewInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_plan_record_review", i)
    }
    #[tool(
        name = "mission_intake_create",
        description = "capability=mission.native-planning version=1. Persist verbatim intake on an existing active Session and idempotently create a managed draft Mission at explicit workspace_root. Normally omit session_root to use workspace_root/.workflow-tools/session; set only to an exact explicitly separate Session store directory, never the repository root. Reuse session_id/request_id and identical input bytes for replay. Returns Mission identity/revision and managed paths in JSON; code!=ok is a tool error, with persisted subsets reported in details. Intake is not approval or execution."
    )]
    async fn intake_create(
        &self,
        Parameters(i): Parameters<Input<named::IntakeInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_intake_create", i)
    }
    #[tool(
        name = "mission_workspace_get",
        description = "capability=mission.native-planning version=1. Read existing managed Mission paths using the original intake session_id/request_id and workspace_root; does not create a Mission. Normally omit session_root; an explicit value selects an exact separate Session store directory, not a repository root. Returns JSON managed paths, digest and generated plan state; a draft may be NotYetGenerated/nonexecutable. code!=ok is a tool error."
    )]
    async fn workspace_get(
        &self,
        Parameters(i): Parameters<Input<named::WorkspaceInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_workspace_get", i)
    }
    #[tool(
        name = "mission_plan_get",
        description = "capability=mission.native-planning version=1. Read plan, lifecycle, reviews, grants, pending challenges and captured decision_refs by Mission UUID and explicit workspace_root. Normally omit session_root; explicit values are exact separate Session store directories, not repo roots. Optional session_id filters only decision_refs, not the plan. Returns JSON code/revision/result/details; code!=ok is a tool error. Use returned revisions/sequences for subsequent checked mutations."
    )]
    async fn plan_get(
        &self,
        Parameters(i): Parameters<Input<named::ReadInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_plan_get", i)
    }
    #[tool(
        name = "mission_plan_update",
        description = "capability=mission.native-planning version=1. Apply tagged set_goal, set_requirements (complete replacement), or record_evidence (record only, never execute) at explicit workspace_root. Requires current expected_revision and stable request_id; changed replay payloads conflict. Semantic edits require fresh review/approval. Returns JSON updated revision/result or code!=ok tool error."
    )]
    async fn plan_update(
        &self,
        Parameters(i): Parameters<Input<named::PlanInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_plan_update", i)
    }
    #[tool(
        name = "mission_waypoint_create",
        description = "capability=mission.native-planning version=1. Create a complete typed waypoint with matching stable local/Part identity, current expected_revision and stable request_id. after inserts after an existing local waypoint; omit/null appends. Provider validates serial route and references. Semantic changes require fresh review/approval; no execution. Returns JSON revision/result or code!=ok tool error."
    )]
    async fn waypoint_create(
        &self,
        Parameters(i): Parameters<Input<named::CreateInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_waypoint_create", i)
    }
    #[tool(
        name = "mission_waypoint_update",
        description = "capability=mission.native-planning version=1. Replace the complete existing typed waypoint identified by waypoint.waypoint_id, not a partial patch. Requires stable request_id and current expected_revision. Provider validates route/references; semantic changes require fresh review/approval. Returns JSON revision/result or code!=ok tool error; no execution."
    )]
    async fn waypoint_update(
        &self,
        Parameters(i): Parameters<Input<named::UpdateInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_waypoint_update", i)
    }
    #[tool(
        name = "mission_waypoint_remove",
        description = "capability=mission.native-planning version=1. Remove an existing local waypoint or replace it with a complete typed waypoint. Omit/null replacement removes only; completed/result-bearing history cannot be removed. Requires stable request_id/current expected_revision and provider route validation. Semantic changes require fresh review/approval. Returns JSON revision/result or code!=ok tool error."
    )]
    async fn waypoint_remove(
        &self,
        Parameters(i): Parameters<Input<named::RemoveInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_waypoint_remove", i)
    }
    #[tool(
        name = "mission_waypoint_reorder",
        description = "capability=mission.native-planning version=1. Replace route order with the complete ordered list of stable local waypoint IDs, not filenames or cross-Mission URNs. Requires stable request_id/current expected_revision; provider validates membership and serial dependencies. Semantic changes require fresh review/approval. Returns JSON revision/result or code!=ok tool error."
    )]
    async fn waypoint_reorder(
        &self,
        Parameters(i): Parameters<Input<named::ReorderInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_waypoint_reorder", i)
    }
    #[tool(
        name = "mission_waypoint_complete",
        description = "capability=mission.native-planning version=1. Complete a local waypoint through approval, outcome evidence, prerequisite, linked Ticket and revision/control-sequence guards. Uses canonical workspace_root/.workflow-tools/session, not an intake-time separate store. request_id is a non-nil UUID. Replays retain stale/conflict rejection without duplicate completion. Returns JSON lifecycle result or code!=ok tool error."
    )]
    async fn waypoint_complete(
        &self,
        Parameters(i): Parameters<Input<named::CompleteInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_waypoint_complete", i)
    }
    #[tool(
        name = "mission_plan_prepare_review",
        description = "capability=mission.native-planning version=1. After passing current cold/formal reviews and complete published output/history integrity checks, persist a challenge bound to the reviewed output contract and journal bytes and return exact host decision prompts. Integrity failure never issues a challenge. request_id is challenge identity, never approval; host capture must supply the later receipt. Normally omit session_root; explicit values are exact separate Session store directories, not repo roots. Returns JSON challenge/revision/digests/prompts or code!=ok tool error; partial registration requires same-identity recovery."
    )]
    async fn prepare_review(
        &self,
        Parameters(i): Parameters<Input<named::ReviewInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_plan_prepare_review", i)
    }
    #[tool(
        name = "mission_plan_approve",
        description = "capability=mission.native-planning version=1. Recheck complete published output/history integrity and reviewed contract/journal bindings before consuming a captured Session host approve receipt bound to current expected_revision/reviews. Integrity failure never grants approval. request_id must equal decision.challenge_id. Actor text, booleans and raw decisions never grant authority. Normally omit session_root; explicit values are exact separate Session store directories, not repo roots. Returns JSON accepted grant/replay status or code!=ok tool error."
    )]
    async fn approve(
        &self,
        Parameters(i): Parameters<Input<named::DecisionInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_plan_approve", i)
    }
    #[tool(
        name = "mission_plan_replan",
        description = "capability=mission.native-planning version=1. Consume a captured Session host replan receipt at current expected_revision, never execution approval. request_id must equal decision.challenge_id. Normally omit session_root; explicit values are exact separate Session store directories, not repo roots. Subsequent semantic edits require fresh review/approval. Returns JSON receipt/replay result or code!=ok tool error."
    )]
    async fn replan(
        &self,
        Parameters(i): Parameters<Input<named::DecisionInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_plan_replan", i)
    }
    #[tool(
        name = "mission_pause",
        description = "capability=mission.native-planning version=1. Pause Mission eligibility without mutating linked Ticket state. Requires non-nil UUID request_id, current expected_revision/expected_control_sequence and nonempty evidence. Uses canonical workspace_root/.workflow-tools/session. Returns JSON lifecycle snapshot or code!=ok tool error."
    )]
    async fn pause(
        &self,
        Parameters(i): Parameters<Input<named::ControlInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_pause", i)
    }
    #[tool(
        name = "mission_resume",
        description = "capability=mission.native-planning version=1. Resume Mission eligibility through current approval and prerequisite checks; requires non-nil UUID request_id, current expected_revision/expected_control_sequence and nonempty evidence. Uses canonical workspace_root/.workflow-tools/session. Returns JSON lifecycle snapshot or code!=ok tool error."
    )]
    async fn resume(
        &self,
        Parameters(i): Parameters<Input<named::ControlInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_resume", i)
    }
    #[tool(
        name = "mission_waypoint_activate",
        description = "capability=mission.native-planning version=1. Activate a Ticket-backed waypoint by transitioning its linked Ticket to in-implementation through WorkspaceLifecycle approval, prerequisite and Ticket legal-state checks. Requires non-nil UUID request_id and current expected_revision checked under the approval lock. Optional author is Ticket history metadata, never approval authority. Uses canonical workspace_root/.workflow-tools/session. Single-session/child-Mission waypoints are unsupported. This is a Ticket transition, not a Mission resume or journaled control: no control_sequence/evidence field and no idempotent replay guarantee. Read the returned lifecycle snapshot before retrying. Returns JSON snapshot or code!=ok tool error."
    )]
    async fn waypoint_activate(
        &self,
        Parameters(i): Parameters<Input<named::ActivateInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_waypoint_activate", i)
    }
    #[tool(
        name = "mission_intent",
        description = "capability=mission.native-planning version=1. Manage schema-v4 requester intent independently of plan objectives. Actions: prepare_baseline, confirm_baseline, prepare_amendment, append_amendment, get. Prepare creates a Session confirmation challenge; confirmation requires a trusted captured host receipt referenced by Session/challenge IDs. No caller-supplied receipt or approval flag. Uses workspace_root/.workflow-tools/session; mutations require expected_revision."
    )]
    async fn intent(
        &self,
        Parameters(i): Parameters<Input<named::IntentInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_intent", i)
    }
    #[tool(
        name = "mission_handoff",
        description = "capability=mission.native-planning version=1. Transfer active binding from verified source_session to existing destination_session through journaled handoff and managed pin recovery. Requires non-nil UUID request_id/current plan and control sequence/nonempty evidence. Uses canonical workspace_root/.workflow-tools/session. Returns JSON lifecycle snapshot or code!=ok tool error."
    )]
    async fn handoff(
        &self,
        Parameters(i): Parameters<Input<named::HandoffInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_handoff", i)
    }
    #[tool(
        name = "mission_cancel",
        description = "capability=mission.native-planning version=1. Cancel Mission eligibility without cascading linked Ticket edits. Requires non-nil UUID request_id, current expected_revision/expected_control_sequence and nonempty evidence. Uses canonical workspace_root/.workflow-tools/session. Returns JSON lifecycle snapshot or code!=ok tool error."
    )]
    async fn cancel(
        &self,
        Parameters(i): Parameters<Input<named::ControlInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_cancel", i)
    }
    #[tool(
        name = "mission_complete",
        description = "capability=mission.native-planning version=1. Explicitly complete a Mission only with current approval, all current outcomes and result review. Requires non-nil UUID request_id/current plan and control sequence/nonempty evidence. Uses canonical workspace_root/.workflow-tools/session. Returns JSON lifecycle snapshot or code!=ok tool error."
    )]
    async fn complete(
        &self,
        Parameters(i): Parameters<Input<named::ControlInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_complete", i)
    }
    #[tool(
        name = "mission_review_results_add_waypoints",
        description = "capability=mission.native-planning version=1. Persist lifecycle result review, then ordered typed waypoint additions, each with its own request_id/expected_revision. Requires non-nil outer UUID request_id/current control sequence/evidence. Uses canonical workspace_root/.workflow-tools/session. Partial errors report persisted review/additions in JSON details; retry only failed additions with mission_waypoint_create. Semantic changes require fresh review/approval. code!=ok is a tool error."
    )]
    async fn review_results(
        &self,
        Parameters(i): Parameters<Input<named::ResultsInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_review_results_add_waypoints", i)
    }
    #[tool(
        name = "mission_reopen_most_recent_completed",
        description = "capability=mission.native-planning version=1. Reopen the greatest current completion sequence, not final route position, with audited linked Ticket transition and approval invalidation. Requires non-nil UUID request_id/current plan, control and completion sequences/nonempty evidence. Uses canonical workspace_root/.workflow-tools/session. Returns JSON lifecycle snapshot or code!=ok tool error."
    )]
    async fn reopen(
        &self,
        Parameters(i): Parameters<Input<named::ReopenInput>>,
    ) -> Result<CallToolResult, McpError> {
        self.named("mission_reopen_most_recent_completed", i)
    }
    #[tool(
        name = "mission_operation",
        description = "Compatibility multiplexer using the server launch workspace (--workspace, then MISSION_WORKSPACE, then cwd), unlike named tools' explicit workspace_root. Run typed create_draft, save_draft, publish_draft, get_mission_source, render_mission; legacy create_mission, edit_mission_objective, add_waypoint, edit_waypoint, order_waypoints, record_progress, record_validation_result, validate_mission, publish_mission; get/get_mission, validate-preview, import, render-preview, publish, check-generated, migrate-dossiers; repair_preview/repair_apply/repair_journal/repair_resume/repair_rollback; publish-with-inputs, get-output-contract, record-approval-event, get-approval-event. See field descriptions for operation-specific required inputs and effects. Typed drafts are inline source, not paths. import accepts without projections; publish generates them. dry_run applies only to migration and defaults false. Reactions/progress are not native requester approval or execution authority. Returns JSON StatusSnapshot: inspect status and diagnostics even when MCP isError is absent; invalid operation/missing fields return invalid_params. Prefer named tools for native planning/lifecycle."
    )]
    async fn operation(
        &self,
        Parameters(input): Parameters<MissionOperationInput>,
    ) -> Result<CallToolResult, McpError> {
        let required_id = || {
            input
                .mission_id
                .as_deref()
                .ok_or_else(|| McpError::invalid_params("mission_id is required", None))
                .and_then(|value| {
                    Uuid::parse_str(value)
                        .map_err(|_| McpError::invalid_params("mission_id must be a UUID", None))
                })
        };
        let required_manifest = || {
            input
                .manifest_path
                .clone()
                .ok_or_else(|| McpError::invalid_params("manifest_path is required", None))
        };
        let required_journal_id = || {
            input
                .journal_id
                .as_deref()
                .ok_or_else(|| McpError::invalid_params("journal_id is required", None))
                .and_then(|value| {
                    Uuid::parse_str(value)
                        .map_err(|_| McpError::invalid_params("journal_id must be a UUID", None))
                })
        };
        let required = |value: &Option<String>, name| {
            value
                .clone()
                .ok_or_else(|| McpError::invalid_params(format!("{name} is required"), None))
        };
        let waypoint = || {
            Ok(DirectWaypoint::from(McpWaypoint {
                waypoint_id: required(&input.waypoint_id, "waypoint_id")?,
                title: required(&input.waypoint_title, "waypoint_title")?,
                depends_on: input.depends_on.clone(),
                prompt: required(&input.prompt, "prompt")?,
                validation: input
                    .validation
                    .clone()
                    .ok_or_else(|| McpError::invalid_params("validation is required", None))?,
                commit_checkpoint: input.commit_checkpoint.clone().ok_or_else(|| {
                    McpError::invalid_params("commit_checkpoint is required", None)
                })?,
                non_goals: input.non_goals.clone(),
            }))
        };
        let operation =
            match input.operation.as_str() {
                "create_draft" => Operation::CreateDraft {
                    title: required(&input.title, "title")?,
                    objective: required(&input.objective, "objective")?,
                },
                "save_draft" => Operation::SaveDraft {
                    mission_id: required_id()?,
                    draft: input
                        .draft
                        .clone()
                        .ok_or_else(|| McpError::invalid_params("draft is required", None))?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "publish_draft" => Operation::PublishDraft {
                    mission_id: required_id()?,
                    draft: input
                        .draft
                        .clone()
                        .ok_or_else(|| McpError::invalid_params("draft is required", None))?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "intent" => Operation::Intent {
                    mission_id: required_id()?,
                    operation: input.intent_action.ok_or_else(|| {
                        McpError::invalid_params("intent_action is required", None)
                    })?,
                },
                "get_mission_source" => Operation::GetMissionSource {
                    mission_id: required_id()?,
                },
                "render_mission" => Operation::RenderMission {
                    mission_id: required_id()?,
                },
                "create_mission" => Operation::CreateMission {
                    title: required(&input.title, "title")?,
                    objective: required(&input.objective, "objective")?,
                    waypoints: input
                        .waypoints
                        .clone()
                        .ok_or_else(|| McpError::invalid_params("waypoints is required", None))?
                        .into_iter()
                        .map(DirectWaypoint::from)
                        .collect(),
                },
                "edit_mission_objective" => Operation::EditMissionObjective {
                    mission_id: required_id()?,
                    objective: required(&input.objective, "objective")?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "add_waypoint" => Operation::AddWaypoint {
                    mission_id: required_id()?,
                    waypoint: waypoint()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "edit_waypoint" => Operation::EditWaypoint {
                    mission_id: required_id()?,
                    waypoint: waypoint()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "order_waypoints" => Operation::OrderWaypoints {
                    mission_id: required_id()?,
                    execution_order: input.execution_order.clone(),
                    waypoint_dependencies: input.waypoint_dependencies.clone(),
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "record_progress" => Operation::RecordProgress {
                    mission_id: required_id()?,
                    waypoint_id: required(&input.waypoint_id, "waypoint_id")?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                    state: match required(&input.state, "state")?.as_str() {
                        "pending" => WaypointState::Pending,
                        "in-progress" => WaypointState::InProgress,
                        "completed" => WaypointState::Completed,
                        "blocked" => WaypointState::Blocked,
                        _ => return Err(McpError::invalid_params("invalid state", None)),
                    },
                },
                "record_validation_result" => Operation::RecordValidationResult {
                    mission_id: required_id()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                    result: OperatorResult {
                        waypoint_id: required(&input.waypoint_id, "waypoint_id")?,
                        outcome: required(&input.outcome, "outcome")?,
                        operator_command: required(&input.operator_command, "operator_command")?,
                        evidence_pointer: required(&input.evidence_pointer, "evidence_pointer")?,
                    },
                },
                "validate_mission" => Operation::ValidateMission {
                    mission_id: required_id()?,
                },
                "publish_mission" => Operation::PublishMission {
                    mission_id: required_id()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "get" | "get_mission" => Operation::Get {
                    mission_id: required_id()?,
                },
                "validate-preview" => Operation::ValidatePreview {
                    manifest_path: required_manifest()?,
                },
                "import" => Operation::Import {
                    mission_id: required_id()?,
                    manifest_path: required_manifest()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "render-preview" => Operation::RenderPreview {
                    mission_id: required_id()?,
                },
                "publish" => Operation::Publish {
                    mission_id: required_id()?,
                    manifest_path: required_manifest()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                },
                "check-generated" => Operation::CheckGenerated {
                    mission_id: required_id()?,
                },
                "get-output-contract" => Operation::GetOutputContract {
                    mission_id: required_id()?,
                },
                "get-approval-event" => Operation::GetApprovalEvent {
                    mission_id: required_id()?,
                    revision: input
                        .revision
                        .ok_or_else(|| McpError::invalid_params("revision is required", None))?,
                },
                "record-approval-event" => Operation::RecordApprovalEvent {
                    mission_id: required_id()?,
                    json: required(&input.approval_event_json, "approval_event_json")?,
                },
                "publish-with-inputs" => Operation::PublishWithInputs {
                    mission_id: required_id()?,
                    manifest_path: required_manifest()?,
                    expected_current_revision: input.expected_current_revision.ok_or_else(
                        || McpError::invalid_params("expected_current_revision is required", None),
                    )?,
                    inputs: input.output_inputs.clone().ok_or_else(|| {
                        McpError::invalid_params("output_inputs is required", None)
                    })?,
                },
                "migrate-dossiers" => Operation::MigrateDossiers {
                    dossier_path: input.dossier_path.clone(),
                    dry_run: input.dry_run,
                },
                "repair_preview" => Operation::RepairPreview {
                    request: input.repair_request.clone().ok_or_else(|| {
                        McpError::invalid_params("repair_request is required", None)
                    })?,
                },
                "repair_apply" => Operation::RepairApply {
                    request: input.repair_request.clone().ok_or_else(|| {
                        McpError::invalid_params("repair_request is required", None)
                    })?,
                    proposal_id: input
                        .proposal_id
                        .clone()
                        .ok_or_else(|| McpError::invalid_params("proposal_id is required", None))?,
                },
                "repair_journal" => Operation::RepairJournal {
                    journal_id: required_journal_id()?,
                },
                "repair_resume" => Operation::RepairResume {
                    journal_id: required_journal_id()?,
                },
                "repair_rollback" => Operation::RepairRollback {
                    journal_id: required_journal_id()?,
                },
                _ => return Err(McpError::invalid_params("unknown operation", None)),
            };
        let snapshot = execute_mcp(&self.workspace, operation);
        let text = serde_json::to_string(&snapshot)
            .map_err(|error| McpError::internal_error(error.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}

#[tool_handler]
impl ServerHandler for MissionServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            instructions: Some(
                "Mission roadmap operations use the canonical workspace store.".into(),
            ),
            ..Default::default()
        }
    }
}

fn main() -> Result<(), HarnessError> {
    let launch = match Launch::try_parse_from(std::env::args_os()) {
        Ok(launch) => launch,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp
                    | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
                    | ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return Ok(());
        }
        Err(error) => return Err(HarnessError::Arguments(error.to_string())),
    };
    let cwd = std::env::current_dir().map_err(HarnessError::Io)?;
    let configured = std::env::var_os(MISSION_WORKSPACE_ENV);
    let (workspace, _) =
        resolve_workspace(launch.workspace.as_deref(), configured.as_deref(), &cwd)
            .map_err(HarnessError::Arguments)?;
    transport_harness::mcp::run(MissionServer::new(workspace))
}
