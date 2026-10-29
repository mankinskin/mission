#[path = "../src/lib.rs"]
mod mission_api;

use mission_api::*;
use serde_json::json;
use spec_api::{AcceptanceCriterion, SpecManifest, SpecStore};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use test_api::{TestStoreConfig, ValidationExecution};
use ticket_api::{model::ticket::TicketManifest, storage::ticket_fs::TicketFs};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct Seed {
    id: &'static str,
    title: &'static str,
    objective: &'static str,
    constraints: &'static str,
    validation: &'static str,
    non_goal: &'static str,
    expected: &'static str,
    alternative: &'static str,
}

macro_rules! seed {
    ($id:literal, $title:literal, $objective:literal, $constraints:literal,
     $validation:literal, $non_goal:literal, $expected:literal, $alternative:literal) => {
        Seed {
            id: $id,
            title: $title,
            objective: $objective,
            constraints: $constraints,
            validation: $validation,
            non_goal: $non_goal,
            expected: $expected,
            alternative: $alternative,
        }
    };
}

const DOC_SEEDS: &[Seed] = &[
    seed!(
        "doc-contract",
        "Define the doc-comment contract",
        "Specify concise purpose, behavior or responsibility, intent, relationships, and constraints for each discoverable Rust item kind.",
        "Distinguish useful explanation from implementation restatement; define structure, length, unsupported kinds, and justified exceptions.",
        "Review representative functions, types, constants, traits, implementations, and modules without inventing behavior.",
        "Do not document the whole workspace or refactor source.",
        "A reviewed schema states required content and safe omission cases for each item kind.",
        "A trait method appears redundant but its behavior or exception cannot be verified."
    ),
    seed!(
        "health-check",
        "Build the documentation health check",
        "Inventory selected modules and report missing, malformed, vague, overlong, or unsupported documentation with stable status over time.",
        "Every finding identifies the item, location, violated contract rule, and whether the case is intentionally undocumented or unsupported.",
        "Run compliant, missing, overlong, malformed, vague-purpose, and justified-exception fixtures.",
        "Do not silently rewrite comments or modify Rust source.",
        "Every fixture item receives a status and actionable rule-specific finding.",
        "A parser cannot classify one item kind reliably."
    ),
    seed!(
        "code-map",
        "Extract maps from verified rustdoc data",
        "Generate normalized records and per-module maps plus an aggregate index of names, paths, descriptions, and signatures.",
        "Evaluate rustdoc JSON first; preserve module identity and report unsupported items and extraction gaps explicitly.",
        "Run Cargo doc for representative crates and compare names, locations, descriptions, signatures, and gap reports.",
        "Do not replace source code, infer missing semantics from HTML, or perform duplicate analysis here.",
        "Maps preserve identity and disclose unavailable fields rather than guessing.",
        "Rustdoc JSON omits a signature field required by a consumer."
    ),
    seed!(
        "semantic-review",
        "Review semantic duplicate candidates",
        "Compare normalized documented intent to find likely redundant behavior and missing abstraction opportunities.",
        "Similarity narrows the search; evidence must retain element links and candidates remain distinct from confirmed redundancy.",
        "Compare exact duplicates, related-but-distinct items, and overlapping vocabulary with unrelated behavior.",
        "Do not automatically merge, delete, generalize, or mutate implementations.",
        "Reports expose evidence-backed candidates for a human-reviewed refactoring decision.",
        "Similar descriptions hide a meaningful behavioral difference."
    ),
];

const MISSION_SEEDS: &[Seed] = &[
    seed!(
        "mission-spec",
        "Define the mission-domain contract",
        "Specify the UUID manifest, objective, roadmap, waypoints, linked domain records, session attachments, store, mutation/read-back invariants, and migration.",
        "Keep source dossiers authoritative until selected migration; preserve the domain crate boundary and exclude scheduling, HTTP, and viewer scope.",
        "Read back the canonical specification and verify ownership, persistence, references, migration, and non-scheduling acceptance criteria.",
        "Do not implement crates, transports, migration code, or scheduling in the specification waypoint.",
        "One authoritative contract names each record owner and keeps sessions and foreign entities reference-only.",
        "A proposed entity reference crosses a store boundary the contract does not define."
    ),
    seed!(
        "ticket-set",
        "Prepare bounded implementation Tickets",
        "Create exactly one Ticket for each ticket-backed API, facade, session-attachment, and migration surface.",
        "Keep W3 alignment, W8 guidance, and W9 closure single-session; preserve status authority in each linked Ticket and reconcile the serial prerequisite chain.",
        "Read each exact Ticket ID, objective, acceptance, Spec link, and dependency edge.",
        "Do not activate implementation before all plan references are read back or create a scheduling Ticket.",
        "W4-W7 each have one linked bounded Ticket; non-ticket work remains untracked by replacement entities.",
        "A Ticket objective, acceptance, or stored edge differs from the reviewed scope."
    ),
    seed!(
        "roadmap-contract",
        "Settle the structured roadmap contract",
        "Define native and typed JSON, stable IDs, explicit serial order, actual dependency edges, revision-local labels, diagnostics, rendering, and recovery.",
        "Do not infer source prerequisites from presentation order, weaken identity checks, or publish a schema change without approval.",
        "Round-trip representative JSON and compare identity, order, dependencies, diagnostics, versioning, and renderer output.",
        "Do not migrate dossiers or implement an unapproved schema.",
        "The contract separates required identity/order from authored prerequisites and defines versioned publication recovery.",
        "A malformed stable ID, ambiguous edge, or unsupported revision is discovered."
    ),
    seed!(
        "mission-api",
        "Implement durable Mission persistence",
        "Implement UUID-backed validation, import, revision, archive, provenance, relationship resolution, and complete generated output in the internal API.",
        "Use `.workflow-tools/mission/`; preserve typed Parts and original source records; keep transports out of the internal crate.",
        "Run focused persistence, validation, revision, exact renderer, history, and rejected-write-preservation tests.",
        "Do not expose transports or mutate source dossiers.",
        "Rejected writes preserve complete accepted bytes and every published revision remains self-contained and readable.",
        "A revision conflicts with current accepted authority or an external reference is unavailable."
    ),
    seed!(
        "mission-transports",
        "Expose the Mission facade",
        "Provide structured-roadmap validate/import/render/publish/check-generated operations over one Mission API through CLI and MCP.",
        "Preserve explicit workspace selection, preview-versus-mutation boundaries, diagnostic parity, and recovery behavior.",
        "Build CLI and MCP and compare representative responses, output destinations, and no-write previews.",
        "Do not add HTTP, viewer, session attachment, or scheduling.",
        "Both transports use shared validation and the selected generated directory without bypasses.",
        "A transport cannot preserve a required diagnostic or mutation boundary."
    ),
    seed!(
        "session-attachment",
        "Attach sessions to Missions",
        "Pin and resolve canonical mission URNs across sessions without embedding the Mission or moving the session-local workflow graph.",
        "Attachment is reference metadata only, not scheduling, shared graph ownership, or permission to mutate a Mission.",
        "Exercise valid pins, invalid URNs, and two sessions referencing one Mission with isolated stores.",
        "Do not move `session_workflow_*` APIs or dispatch agents.",
        "Exact references resolve while each session retains its independent local graph.",
        "A referenced Mission is missing, malformed, or selected from the wrong workspace."
    ),
    seed!(
        "dossier-migration",
        "Migrate selected dossiers safely",
        "Enumerate source folders at runtime, import only the current roadmap, preserve provenance and current links, and report migrated/skipped/blocked inputs.",
        "Leave original dossiers and version snapshots unchanged; derive IDs only by the approved title rule and defer unsupported legacy cases explicitly.",
        "Compare dry-run inventory to filesystem contents, inspect the migrated dossier record, and verify all excluded snapshot paths.",
        "Do not delete source dossiers, impose historical folder counts, or convert every legacy folder.",
        "Eligible current roadmaps are represented while blocked folders and excluded history remain visible and auditable.",
        "A source folder is ambiguous, unsafe, or outside the approved migration boundary."
    ),
    seed!(
        "guidance-retarget",
        "Retarget authorized guidance",
        "Replace only approved dossier routes with durable Mission operations after migration is validated.",
        "Require explicit requester authorization for takeover regardless of prior session status; preserve prior/new owner provenance and avoid unrelated policy edits.",
        "Search only owned guidance for stale routes and verify every changed link and takeover boundary.",
        "Do not rewrite unrelated instructions or infer authority from expiry, pins, or history.",
        "Guidance routes to Mission workflows without weakening the explicit takeover gate.",
        "A guidance owner, target, or takeover authorization cannot be verified."
    ),
    seed!(
        "validation-closure",
        "Close with evidence and dispositions",
        "Read back the specification, implementation Tickets, validation executions, migration outcome, and required requester dispositions.",
        "A green test is not review or requester acceptance; record the approved blocked backlog separately from unresolved exceptions.",
        "Verify evidence links, exact entity status, generated output, and closure disposition before the final lifecycle transition.",
        "Do not close with missing gates, unresolved feedback, or unapproved scope.",
        "Closure follows linked evidence and explicit dispositions, not a summary claim.",
        "A blocked item has no exact requester-approved disposition or validation read-back."
    ),
];

const TICKET_SEEDS: &[Seed] = &[
    seed!(
        "part-write",
        "Establish the part-write contract",
        "Reject blank Ticket-part content and preserve settled lifecycle constraints for writes.",
        "Keep the contract narrow; transaction recovery and migration are separate work.",
        "Run focused Ticket API validation and inspect empty, whitespace, and valid part cases.",
        "Do not change unrelated lifecycle states.",
        "Blank writes fail while valid part content and state rules remain stable.",
        "A whitespace-only payload reaches a transport path not covered by the first check."
    ),
    seed!(
        "recoverable-publication",
        "Make filesystem publication recoverable",
        "Stage part bytes and manifest updates with a recoverable transaction and explicit rollback state.",
        "A multi-file operation needs a journal; a single rename is not the transaction.",
        "Inject failures around stage, backup, replace, and recovery; verify old or new complete state.",
        "Do not add migration selection or broad cleanup.",
        "Every interrupted publication is recoverable without a mixed manifest/file state.",
        "A recovery journal indicates a partial operation whose inputs need inspection."
    ),
    seed!(
        "residual-test-scope",
        "Decide residual package failures",
        "Separate missing server behavior from fixture defects before changing the full package test gate.",
        "Keep the gate intact and do not create Mission entities for this Ticket task.",
        "Reproduce each failure and record the scoped decision before implementation.",
        "Do not implement server behavior or weaken the package gate in this decision step.",
        "Each residual failure has an evidence-backed owner and an explicit disposition.",
        "A failure cannot be attributed safely from the current fixture evidence."
    ),
    seed!(
        "missing-prefix",
        "Report missing-prefix search scope",
        "Return useful searched-workspace diagnostics when a Ticket identifier prefix matches nothing.",
        "Diagnostics are read-only and must reveal no unrelated Ticket content.",
        "Exercise absent and ambiguous prefixes under explicit workspace roots.",
        "Do not broaden the resolver to arbitrary ancestor stores.",
        "The result names the search scope and leaves every store byte unchanged.",
        "A prefix matches a candidate outside the selected workspace."
    ),
    seed!(
        "descendant-refs",
        "Resolve descendant Ticket references",
        "Allow a selected workspace to follow valid Ticket references into owned descendants.",
        "Validate canonical IDs and remain within the explicitly selected store.",
        "Test valid descendant references, absent paths, and traversal attempts.",
        "Do not accept arbitrary external stores.",
        "Valid local references resolve and invalid paths fail before writes.",
        "A symlinked descendant escapes the canonical root."
    ),
    seed!(
        "default-workspace",
        "Honor default-workspace reads",
        "Permit read-only operations to use the documented workspace default when a caller omits it.",
        "Keep writes explicit and prevent omission from selecting an unrelated store.",
        "Compare omitted and explicit read behavior and assert write rejection without root.",
        "Do not change production workspace policy.",
        "Read queries use the same selected default and writes retain explicit-root requirements.",
        "Default resolution is ambiguous between two candidate workspaces."
    ),
    seed!(
        "board-fixtures",
        "Repair board MCP workspace fixtures",
        "Make board tests construct data under the workspace root actually resolved by the server.",
        "Fix fixtures rather than changing valid production lookup behavior.",
        "Run the focused board MCP suite with exact root read-back.",
        "Do not relax workspace scoping or touch live board state.",
        "Tests read their own generated board records from the selected root.",
        "A test unexpectedly discovers a host-level board store."
    ),
    seed!(
        "move-fixtures",
        "Repair move MCP workspace fixtures",
        "Align move preflight/apply tests with the configured source and destination stores.",
        "Keep move operations isolated and verify rollback on rejection.",
        "Exercise same-root, distinct-root, and stale-report fixtures.",
        "Do not perform a live store move.",
        "Each fixture proves the intended source/destination pair and preserves unrelated bytes.",
        "The report's source identity no longer matches the fixture root."
    ),
    seed!(
        "index-output",
        "Align store-index output fixtures",
        "Place generated index artifacts beneath the canonical Ticket store paths used by tests.",
        "Test paths must match `.workflow-tools/ticket` and stay inside their fixture root.",
        "Compare exact generated artifact paths and rerun the focused index suite.",
        "Do not change the index contract to fit an incorrect fixture.",
        "The test observes the canonical artifact location and stable contents.",
        "An output is created outside the expected store root."
    ),
    seed!(
        "update-fixture",
        "Correct update MCP workspace fixtures",
        "Ensure updates target the same explicit fixture workspace used for setup and read-back.",
        "Keep mutation scoped to the synthetic Ticket and verify exact identity after update.",
        "Run update MCP tests against a freshly initialized local store.",
        "Do not update any real Ticket.",
        "The returned and persisted Ticket share the intended canonical ID.",
        "Read-back resolves a different workspace than setup."
    ),
    seed!(
        "test-gate",
        "Repair the full Ticket test gate",
        "Correct persisted test fixtures to valid schema states so the full package gate reaches the behaviors under test.",
        "Normalize only invalid fixture setup; retain legacy values only where a test explicitly covers scan compatibility.",
        "Run the full Ticket package and formatting checks after focused fixture corrections.",
        "Do not weaken production state validation or change transport behavior.",
        "The complete package suite passes against schema-valid state fixtures.",
        "A remaining failure is outside invalid fixture setup and requires separate scope."
    ),
    seed!(
        "part-layout",
        "Scope part-layout migration",
        "Select only existing manifest-referenced part files eligible for canonical naming or cleanup.",
        "Never synthesize parts from flat legacy fields; retain stable IDs and order.",
        "Test duplicate kinds, frozen empty parts, missing files, and untouched legacy content.",
        "Do not rewrite unreferenced files or migrate outside the chosen stores.",
        "The candidate set is deterministic and every excluded file remains byte-identical.",
        "A candidate has ambiguous provenance or no manifest reference."
    ),
    seed!(
        "cli-dispatch",
        "Correct migration CLI dispatch",
        "Ensure the migration command reaches the selected store before producing a dry-run report.",
        "Preserve the exact workspace and operation mode through argument dispatch.",
        "Run the focused dispatch test and assert a same-root report.",
        "Do not apply or repair migration data here.",
        "Dry-run dispatch returns the selected store's report without mutation.",
        "Dispatch resolves a different store root or no handler."
    ),
    seed!(
        "migration-scope",
        "Bound migration scope to selected stores",
        "Compare candidates against every current manifest in the two named stores and no third store.",
        "Treat each candidate's provenance separately; exclude legacy flat-content conversion.",
        "Verify root and nested inventories, target paths, and zero-conflict results.",
        "Do not clean unrelated stores or infer common ownership for zero-byte files.",
        "The exact two-store scope is explicit and reproducible.",
        "The inventory includes a third root or an ineligible record."
    ),
    seed!(
        "root-preflight",
        "Run root-store preflight",
        "Check freshness, candidate identity, and preservation for the selected root and nested Ticket stores.",
        "The root board ownership query fails with OS error 3; retain that uncertainty and do not apply.",
        "Verify both reports, same-root candidate paths, and unchanged store bytes.",
        "Do not claim ownership or authorize live migration.",
        "Preflight evidence is preserved while unresolved root ownership remains a blocker.",
        "The explicit root query still fails or a report is stale."
    ),
    seed!(
        "approved-apply",
        "Apply only separately approved operations",
        "Keep live migration blocked until root ownership is repaired and the exact operation set receives separate approval.",
        "No approval or ownership may be inferred from clean candidate reports.",
        "Before any future apply, require fresh reports, operation-by-operation approval, and immediate read-back.",
        "Do not apply, verify, or undo any live migration in this fixture.",
        "The synthetic example remains blocked with no source or store mutation.",
        "No separate approval exists for the exact operation order."
    ),
    seed!(
        "transport-docs",
        "Integrate transport and settled behavior",
        "Document validated Ticket-part behavior consistently across CLI, MCP, and owned usage guidance.",
        "Retain the two-store boundary and keep evidence/status authorities separate.",
        "Run Ticket API/package and transport checks, then read back each validation record.",
        "Do not create new entities or broaden migration.",
        "Transport behavior and documentation agree with the validated contract.",
        "A transport contract or required validation read-back is missing."
    ),
];

#[derive(Clone, Copy)]
struct FixtureIdentity {
    mission: u128,
    ticket: u128,
    spec: u128,
    criterion: &'static str,
    execution: &'static str,
    title: &'static str,
}

const DOC_IDENTITY: FixtureIdentity = FixtureIdentity {
    mission: 0x00000000000040008000000000001001,
    ticket: 0x00000000000040008000000000001002,
    spec: 0x00000000000040008000000000001003,
    criterion: "doc-comment-review-contract",
    execution: "doc-comment-synthetic-check",
    title: "Synthetic documentation workflow fixture",
};
const MISSION_IDENTITY: FixtureIdentity = FixtureIdentity {
    mission: 0x00000000000040008000000000002001,
    ticket: 0x00000000000040008000000000002002,
    spec: 0x00000000000040008000000000002003,
    criterion: "mission-domain-boundary",
    execution: "mission-integration-synthetic-check",
    title: "Synthetic Mission integration fixture",
};
const TICKET_IDENTITY: FixtureIdentity = FixtureIdentity {
    mission: 0x00000000000040008000000000003001,
    ticket: 0x00000000000040008000000000003002,
    spec: 0x00000000000040008000000000003003,
    criterion: "ticket-migration-safety",
    execution: "ticket-reliability-synthetic-check",
    title: "Synthetic Ticket reliability fixture",
};

#[derive(serde::Serialize, serde::Deserialize)]
struct NativeFixtureInput {
    mission_id: Uuid,
    bundle: MissionBundle,
}

fn outcome(seed: &Seed) -> WaypointOutcomePlan {
    WaypointOutcomePlan {
        expected_outcome: seed.expected.into(),
        alternative_outcomes: vec![AlternativeOutcome {
            outcome: seed.alternative.into(),
            acceptable_handling: vec![
                "Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.".into(),
            ],
        }],
    }
}

fn part_markdown(seed: &Seed) -> String {
    format!(
        "## Adapted objective\n\n{}\n\n## Constraints and boundary\n\n{}\n\n## Validation and non-goal\n\nValidation guidance: {}. This is a planned check, not a claim that the source command was run.\n\nNon-goal: {}.\n\n## Fixture-authored outcome guidance\n\nExpected: {}\n\nAlternative: {}\n\nAcceptable handling: record the condition, preserve source data, and stop affected follow-up.\n",
        seed.objective,
        seed.constraints,
        seed.validation,
        seed.non_goal,
        seed.expected,
        seed.alternative
    )
}

fn fixture_relationships(identity: FixtureIdentity) -> WaypointRelationships {
    WaypointRelationships {
        specification_ids: vec![Uuid::from_u128(identity.spec)],
        criteria: vec![CriterionReference {
            specification_id: Uuid::from_u128(identity.spec),
            criterion_id: CriterionId::new(identity.criterion).unwrap(),
        }],
        executions: vec![WaypointExecutionReference {
            execution_id: identity.execution.into(),
            criterion: None,
        }],
    }
}

fn native_input(
    identity: FixtureIdentity,
    seeds: &[Seed],
    completed: bool,
    blocked_index: Option<usize>,
    pending_index: Option<usize>,
    linked_index: usize,
) -> NativeFixtureInput {
    let ticket_ref = format!("ce://default/ticket/{}", Uuid::from_u128(identity.ticket));
    let waypoints = seeds
        .iter()
        .enumerate()
        .map(|(index, seed)| {
            let linked = index == linked_index;
            WaypointFragment {
                outcome_plan: Some(outcome(seed)),
                waypoint_id: seed.id.into(),
                title: seed.title.into(),
                depends_on: index
                    .checked_sub(1)
                    .map(|previous| vec![seeds[previous].id.into()])
                    .unwrap_or_default(),
                mode: WaypointMode::ExecutionSideEffect,
                scope: if linked {
                    Scope::TicketBacked {
                        ticket_ref: ticket_ref.clone(),
                    }
                } else {
                    Scope::SingleSession
                },
                state: if linked {
                    None
                } else if blocked_index == Some(index) {
                    Some(WaypointState::Blocked)
                } else if pending_index == Some(index) {
                    Some(WaypointState::Pending)
                } else if completed {
                    Some(WaypointState::Completed)
                } else {
                    None
                },
                ticket_ref: linked.then(|| ticket_ref.clone()),
                child_mission_ref: None,
                relationships: if linked {
                    fixture_relationships(identity)
                } else {
                    WaypointRelationships::default()
                },
                session_package: format!("w8-{}", seed.id),
                prompt: format!(
                    "Fixture-only adaptation: {} Preserve the boundary: {}",
                    seed.objective, seed.constraints
                ),
                artifacts: vec![],
                requirement_ids: vec![],
                non_goals: vec![seed.non_goal.into()],
                validation: vec![ValidationGate {
                    command: seed.validation.into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: format!("Fixture review checkpoint: {}", seed.id),
                },
                part_markdown: part_markdown(seed),
            }
        })
        .collect();
    NativeFixtureInput {
        mission_id: Uuid::from_u128(identity.mission),
        bundle: MissionBundle {
            manifest: BundleManifest {
                schema_version: OUTCOME_BUNDLE_SCHEMA_VERSION,
                revision: 1,
                title: identity.title.into(),
                objective: format!(
                    "Synthetic, read-only adaptation of {} waypoints. No source roadmap is executed or mutated.",
                    seeds.len()
                ),
                waypoint_files: vec![],
                execution_order: seeds.iter().map(|seed| seed.id.into()).collect(),
                artifacts: vec![],
                requirements: vec![],
                validation_gates: vec![],
                notes: vec![
                    "Synthetic adaptation. Mission API predecessor checkpoints encode rendered order and do not assert source-roadmap prerequisites.".into(),
                    "Fixture-authored outcome plans and synthetic local evidence are not requester approvals or real execution evidence.".into(),
                ],
                operator_results: if identity.mission == DOC_IDENTITY.mission {
                    vec![OperatorResult {
                        waypoint_id: "code-map".into(),
                        outcome: "passed".into(),
                        operator_command: "fixture-authored observation; source workspace was not scanned".into(),
                        evidence_pointer: format!(
                            "ce://default/test/{}",
                            DOC_IDENTITY.execution
                        ),
                    }]
                } else {
                    vec![]
                },
            },
            waypoints,
        },
    }
}

fn typed_draft(identity: FixtureIdentity) -> TypedDraft {
    let ticket_id = Uuid::from_u128(identity.ticket);
    TypedDraft {
        mission_id: Uuid::from_u128(identity.mission),
        schema_version: OUTCOME_SOURCE_SCHEMA_VERSION,
        revision: 1,
        title: identity.title.into(),
        objective: "Synthetic adaptation of the full Mission integration roadmap; no source work is executed or mutated.".into(),
        waypoints: MISSION_SEEDS
            .iter()
            .map(|seed| TypedWaypoint {
                outcome_plan: Some(outcome(seed)),
                waypoint_id: seed.id.into(),
                title: seed.title.into(),
                prompt: format!(
                    "Fixture-only adaptation: {} Preserve the boundary: {}",
                    seed.objective, seed.constraints
                ),
                part: PartSource {
                    waypoint_id: seed.id.into(),
                    document: Document {
                        blocks: vec![
                            Block::Heading {
                                level: 2,
                                inlines: vec![Inline::Text {
                                    text: "Adapted objective".into(),
                                }],
                            },
                            Block::Paragraph {
                                inlines: vec![Inline::Text {
                                    text: seed.objective.into(),
                                }],
                            },
                            Block::Heading {
                                level: 2,
                                inlines: vec![Inline::Text {
                                    text: "Constraints and validation".into(),
                                }],
                            },
                            Block::Paragraph {
                                inlines: vec![Inline::Text {
                                    text: format!(
                                        "{} Validation guidance: {}. Non-goal: {}.",
                                        seed.constraints, seed.validation, seed.non_goal
                                    ),
                                }],
                            },
                        ],
                    },
                },
                depends_on: vec![],
                validation: vec![ValidationGate {
                    command: seed.validation.into(),
                    cwd: None,
                }],
                commit_checkpoint: CommitCheckpoint {
                    message: format!("Fixture review checkpoint: {}", seed.id),
                },
                non_goals: vec![seed.non_goal.into()],
                ownership_refs: if seed.id == "mission-api" {
                    OwnershipRefs {
                        ticket_id: Some(ticket_id),
                        ..Default::default()
                    }
                } else {
                    OwnershipRefs::default()
                },
                relationships: if seed.id == "mission-api" {
                    fixture_relationships(identity)
                } else {
                    WaypointRelationships::default()
                },
            })
            .collect(),
    }
}

#[test]
#[ignore = "one-time creation of canonical API authoring inputs"]
fn bootstrap_authoring_inputs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/representative-missions/authoring");
    fs::create_dir_all(&root).unwrap();
    for (name, value) in [
        (
            "documentation-workflow.native-v2.json",
            serde_json::to_vec_pretty(&native_input(DOC_IDENTITY, DOC_SEEDS, false, None, None, 3))
                .unwrap(),
        ),
        (
            "mission-integration.typed-v3.json",
            serde_json::to_vec_pretty(&typed_draft(MISSION_IDENTITY)).unwrap(),
        ),
        (
            "ticket-reliability.native-v2.json",
            serde_json::to_vec_pretty(&native_input(
                TICKET_IDENTITY,
                TICKET_SEEDS,
                true,
                Some(15),
                Some(16),
                14,
            ))
            .unwrap(),
        ),
    ] {
        fs::write(root.join(name), value).unwrap();
    }
}

fn bundle() -> MissionBundle {
    MissionBundle {
        manifest: BundleManifest {
            schema_version: SCHEMA_VERSION,
            revision: 1,
            title: "Deterministic fixture".into(),
            objective: "Verify stable event metadata in isolated stores.".into(),
            waypoint_files: vec![],
            execution_order: vec!["verify".into()],
            artifacts: vec![],
            requirements: vec![],
            validation_gates: vec![],
            notes: vec![],
            operator_results: vec![],
        },
        waypoints: vec![WaypointFragment {
            outcome_plan: None,
            waypoint_id: "verify".into(),
            title: "Verify".into(),
            depends_on: vec![],
            mode: WaypointMode::ExecutionSideEffect,
            scope: Scope::SingleSession,
            state: Some(WaypointState::Pending),
            ticket_ref: None,
            child_mission_ref: None,
            relationships: WaypointRelationships::default(),
            session_package: "representative-mission-fixtures".into(),
            prompt: "Verify deterministic test metadata.".into(),
            artifacts: vec![],
            requirement_ids: vec![],
            non_goals: vec![],
            validation: vec![ValidationGate {
                command: "cargo test".into(),
                cwd: None,
            }],
            commit_checkpoint: CommitCheckpoint {
                message: "test(mission-api): verify deterministic fixtures".into(),
            },
            part_markdown: "Synthetic fixture Part.".into(),
        }],
    }
}

fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(path).unwrap(),
                );
            }
        }
    }

    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn test_only_event_metadata_provider_reproduces_complete_event_tree() {
    let mission_id = Uuid::from_u128(0x00000000000040008000000000000001);
    let event_id = Uuid::from_u128(0x00000000000040008000000000000002);
    let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let first_root = tempfile::Builder::new()
        .prefix(".representative-w8-first-")
        .tempdir_in(fixture_root)
        .unwrap();
    let second_root = tempfile::Builder::new()
        .prefix(".representative-w8-second-")
        .tempdir_in(fixture_root)
        .unwrap();

    let first = generate_event_tree(first_root.path(), mission_id, event_id);
    let second = generate_event_tree(second_root.path(), mission_id, event_id);

    assert_eq!(first, second);
}

fn generate_event_tree(
    workspace: &Path,
    mission_id: Uuid,
    event_id: Uuid,
) -> BTreeMap<String, Vec<u8>> {
    let store = MissionStore::init(workspace)
        .unwrap()
        .with_test_event_metadata(event_id, 1_798_000_000_000);
    store.import(mission_id, bundle(), 0).unwrap();
    let binding = store.output_contract_binding(mission_id).unwrap();
    let request = serde_json::json!({
        "revision": 1,
        "decision": "approve",
        "signature": binding,
        "actor": "synthetic reviewer",
        "actor_source": "fixture-authored",
        "metadata": {"claim": "not human approval"}
    });
    store
        .record_approval_event_json(mission_id, request.to_string().as_bytes())
        .unwrap();
    store.verify_output_history(mission_id).unwrap();
    let mission_directory = workspace
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string());
    tree(&mission_directory)
}

#[derive(Default)]
struct ReferenceInventory {
    specification_ids: BTreeSet<Uuid>,
    criteria: BTreeMap<Uuid, BTreeSet<String>>,
    ticket_ids: BTreeSet<Uuid>,
    execution_ids: BTreeSet<String>,
}

impl ReferenceInventory {
    fn add_relationships(&mut self, links: &WaypointRelationships) {
        self.specification_ids
            .extend(links.specification_ids.iter().copied());
        for criterion in &links.criteria {
            self.specification_ids.insert(criterion.specification_id);
            self.criteria
                .entry(criterion.specification_id)
                .or_default()
                .insert(criterion.criterion_id.as_str().into());
        }
        self.execution_ids.extend(
            links
                .executions
                .iter()
                .map(|execution| execution.execution_id.clone()),
        );
    }

    fn add_ticket(&mut self, reference: Option<&str>) {
        if let Some(id) = reference
            .and_then(|value| value.strip_prefix("ce://"))
            .and_then(|value| value.split('/').nth(2))
            .and_then(|value| Uuid::parse_str(value).ok())
        {
            self.ticket_ids.insert(id);
        }
    }
}

fn references_for_bundle(bundle: &MissionBundle) -> ReferenceInventory {
    let mut references = ReferenceInventory::default();
    for waypoint in &bundle.waypoints {
        references.add_relationships(&waypoint.relationships);
        references.add_ticket(waypoint.ticket_ref.as_deref());
        if let Scope::TicketBacked { ticket_ref } = &waypoint.scope {
            references.add_ticket(Some(ticket_ref));
        }
    }
    references
}

fn references_for_draft(draft: &TypedDraft) -> ReferenceInventory {
    let mut references = ReferenceInventory::default();
    for waypoint in &draft.waypoints {
        references.add_relationships(&waypoint.relationships);
        references.add_ticket(waypoint.ownership_refs.ticket_uri.as_deref());
        if let Some(id) = waypoint.ownership_refs.ticket_id {
            references.ticket_ids.insert(id);
        }
    }
    references
}

fn seed_reference_stores(root: &Path, references: &ReferenceInventory) {
    let mut spec_store = SpecStore::init(&root.join(".workflow-tools/spec")).unwrap();
    for id in &references.specification_ids {
        let criteria = references
            .criteria
            .get(id)
            .into_iter()
            .flatten()
            .map(|criterion_id| AcceptanceCriterion {
                id: criterion_id.clone(),
                statement: format!(
                    "Synthetic criterion {criterion_id} for isolated fixture review."
                ),
                expected_property_ids: vec![],
                required_evidence_ids: vec![],
            })
            .collect();
        let mut specification = SpecManifest::new(
            &format!("representative-mission/{id}"),
            "Synthetic representative Mission specification",
            "mission",
        );
        specification.id = *id;
        specification.set_state("accepted");
        specification.set_acceptance_criteria(criteria);
        spec_store
            .create(
                &specification,
                "Synthetic local fixture record. Not a requester-approved specification.",
                None,
            )
            .unwrap();
    }

    for id in &references.ticket_ids {
        let related_specs: Vec<_> = references
            .specification_ids
            .iter()
            .map(|spec_id| {
                json!({
                    "spec_id": spec_id,
                    "workspace": "default",
                    "store_root": ".workflow-tools/spec"
                })
            })
            .collect();
        let ticket: TicketManifest = serde_json::from_value(json!({
            "id": id,
            "created_at": "2026-10-29T00:00:00Z",
            "title": "Synthetic representative Mission Ticket",
            "state": "done",
            "related_specs": related_specs
        }))
        .unwrap();
        TicketFs::create(&ticket, &root.join(".workflow-tools/ticket/tickets"), None).unwrap();
    }

    if !references.execution_ids.is_empty() {
        let store = TestStoreConfig::new(root.join(".workflow-tools/test"));
        let spec_ids: Vec<_> = references
            .specification_ids
            .iter()
            .map(Uuid::to_string)
            .collect();
        let criterion_ids: Vec<_> = references.criteria.values().flatten().cloned().collect();
        for id in &references.execution_ids {
            let execution: ValidationExecution = serde_json::from_value(json!({
                "id": id,
                "validation_spec_id": "representative-mission-synthetic-check",
                "outcome": "passed",
                "executed_at": "2026-10-29T00:00:00Z",
                "detail": "Synthetic fixture evidence; no product validation is claimed.",
                "links": {
                    "spec_ids": spec_ids,
                    "acceptance_criterion_ids": criterion_ids
                },
                "provenance": {
                    "domain": "mission",
                    "operation": "representative-fixture",
                    "run_id": "synthetic-only"
                }
            }))
            .unwrap();
            store.record_execution(&execution).unwrap();
        }
    }
}

fn initial_inputs() -> MissionOutputInputs {
    MissionOutputInputs {
        merged_request: Some("Synthetic request state 1; not source execution or approval.".into()),
        continuations: BTreeMap::new(),
    }
}

fn fixture_paths() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/representative-missions")
}

fn fixture_cases() -> [(&'static str, &'static str); 3] {
    [
        (
            "documentation-workflow",
            "documentation-workflow.native-v2.json",
        ),
        ("mission-integration", "mission-integration.typed-v3.json"),
        ("ticket-reliability", "ticket-reliability.native-v2.json"),
    ]
}

fn generate_fixture(workspace: &Path, authoring: &Path) -> Uuid {
    let authoring_bytes = fs::read(authoring).unwrap();
    let store = MissionStore::init(workspace)
        .unwrap()
        .with_test_event_metadata(
            Uuid::from_u128(0x0000000000004000800000000000f00d),
            1_798_000_000_000,
        );
    if authoring.extension().and_then(|ext| ext.to_str()) == Some("json")
        && authoring
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".native-v2.json"))
    {
        let input: NativeFixtureInput = serde_json::from_slice(&authoring_bytes).unwrap();
        seed_reference_stores(workspace, &references_for_bundle(&input.bundle));
        let mission_id = input.mission_id;
        let expected_bundle = input.bundle;
        let published = store
            .publish_with_inputs(
                mission_id,
                expected_bundle.clone(),
                0,
                &initial_inputs(),
                None,
            )
            .unwrap();
        assert_eq!(published.bundle, expected_bundle);
        mission_id
    } else {
        let draft: TypedDraft = serde_json::from_slice(&authoring_bytes).unwrap();
        seed_reference_stores(workspace, &references_for_draft(&draft));
        let mission_id = draft.mission_id;
        let lifecycle = MissionLifecycle::open(workspace);
        let published_source = lifecycle.publish_draft(mission_id, draft, 0).unwrap();
        assert_eq!(
            lifecycle.get_mission_source(mission_id).unwrap(),
            published_source
        );
        let accepted = store.get(mission_id).unwrap();
        for waypoint in &published_source.waypoints {
            assert_eq!(
                accepted
                    .bundle
                    .waypoints
                    .iter()
                    .find(|accepted| accepted.waypoint_id == waypoint.waypoint_id)
                    .unwrap()
                    .outcome_plan,
                waypoint.outcome_plan
            );
        }

        let first = store.get(mission_id).unwrap();
        let binding = store.output_contract_binding(mission_id).unwrap();
        let reaction = serde_json::json!({
            "revision": 1,
            "decision": "changes-requested",
            "signature": binding,
            "actor": "synthetic fixture reviewer",
            "actor_source": "fixture-authored",
            "metadata": {"notice": "not authentication or human approval"}
        });
        store
            .record_approval_event_json(mission_id, reaction.to_string().as_bytes())
            .unwrap();

        let mut revision_two = first.bundle;
        revision_two.manifest.revision = 2;
        revision_two.manifest.objective.push_str(
            " Revision 2 preserves the source scope while adding synthetic fixture context.",
        );
        let initial_inputs = MissionOutputInputs {
            merged_request: Some("Synthetic REQUEST state 2.".into()),
            continuations: BTreeMap::from([(
                "reviewer/initial.md".into(),
                "Synthetic continuation retained in later whole states.".into(),
            )]),
        };
        store
            .publish_with_inputs(mission_id, revision_two, 1, &initial_inputs, None)
            .unwrap();

        let current = store.get(mission_id).unwrap();
        let mut revision_three = current.bundle;
        revision_three.manifest.revision = 3;
        revision_three.manifest.objective.push_str(
            " Revision 3 records a changed synthetic request without replacing prior continuation.",
        );
        let changed_inputs = MissionOutputInputs {
            merged_request: Some("Changed synthetic REQUEST state 3.".into()),
            continuations: BTreeMap::from([(
                "reviewer/follow-up.md".into(),
                "A separate fixture-authored continuation for the later whole state.".into(),
            )]),
        };
        store
            .publish_with_inputs(mission_id, revision_three, 2, &changed_inputs, None)
            .unwrap();
        mission_id
    }
}

fn generated_directory(workspace: &Path, mission_id: Uuid) -> PathBuf {
    workspace
        .join(".workflow-tools/mission/missions")
        .join(mission_id.to_string())
}

fn assert_diagnostic(error: MissionError, expected_code: &str) {
    let diagnostics = error
        .diagnostics()
        .expect("rejected fixture operation should return diagnostics");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == expected_code),
        "expected diagnostic {expected_code}, got {diagnostics:?}"
    );
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for (relative, bytes) in tree(source) {
        let path = destination.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }
}

#[test]
#[ignore = "explicitly regenerate committed review outputs through Mission APIs"]
fn regenerate_reviewable_outputs() {
    let corpus = fixture_paths();
    let authoring = corpus.join("authoring");
    let generated = corpus.join("generated");
    fs::create_dir_all(&generated).unwrap();
    for (slug, filename) in fixture_cases() {
        let workspace = tempfile::Builder::new()
            .prefix(".representative-w8-generation-")
            .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap();
        let mission_id = generate_fixture(workspace.path(), &authoring.join(filename));
        let output = generated.join(slug);
        if output.exists() {
            fs::remove_dir_all(&output).unwrap();
        }
        copy_tree(&generated_directory(workspace.path(), mission_id), &output);
    }
}

#[test]
fn committed_corpus_is_byte_identical_to_fresh_api_generation() {
    let corpus = fixture_paths();
    for (slug, filename) in fixture_cases() {
        let workspace = tempfile::Builder::new()
            .prefix(".representative-w8-verify-")
            .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
            .unwrap();
        let mission_id =
            generate_fixture(workspace.path(), &corpus.join("authoring").join(filename));
        let store = MissionStore::open(workspace.path()).with_test_event_metadata(
            Uuid::from_u128(0x0000000000004000800000000000f00d),
            1_798_000_000_000,
        );
        let accepted = store.get(mission_id).unwrap();
        assert_eq!(accepted.mission_id, mission_id);
        assert_eq!(
            store.output_contract(mission_id).unwrap().renderer,
            "mission-api/7"
        );
        store.check_generated_output(mission_id).unwrap();
        store.verify_output_history(mission_id).unwrap();
        let canonical_root = workspace.path().canonicalize().unwrap();
        for bytes in tree(&generated_directory(workspace.path(), mission_id)).values() {
            let content = String::from_utf8_lossy(bytes);
            assert!(
                !content.contains(&canonical_root.to_string_lossy().to_string()),
                "generated output leaked its temporary workspace path for {slug}"
            );
        }
        let roadmap = fs::read_to_string(
            generated_directory(workspace.path(), mission_id).join("generated/ROADMAP.md"),
        )
        .unwrap();
        assert!(roadmap.contains(
            "Owning workspace: selected workspace root (local records in .workflow-tools/)."
        ));
        assert_eq!(
            tree(&corpus.join("generated").join(slug)),
            tree(&generated_directory(workspace.path(), mission_id)),
            "complete Mission tree differs for {slug}"
        );
    }
}

#[test]
fn canonical_inputs_cover_native_and_typed_schemas_with_all_outcomes() {
    let corpus = fixture_paths();
    let documentation: NativeFixtureInput = serde_json::from_slice(
        &fs::read(corpus.join("authoring/documentation-workflow.native-v2.json")).unwrap(),
    )
    .unwrap();
    let ticket: NativeFixtureInput = serde_json::from_slice(
        &fs::read(corpus.join("authoring/ticket-reliability.native-v2.json")).unwrap(),
    )
    .unwrap();
    let integration: TypedDraft = serde_json::from_slice(
        &fs::read(corpus.join("authoring/mission-integration.typed-v3.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(
        documentation.bundle.manifest.schema_version,
        OUTCOME_BUNDLE_SCHEMA_VERSION
    );
    assert_eq!(documentation.bundle.waypoints.len(), DOC_SEEDS.len());
    assert_eq!(
        ticket.bundle.manifest.schema_version,
        OUTCOME_BUNDLE_SCHEMA_VERSION
    );
    assert_eq!(ticket.bundle.waypoints.len(), TICKET_SEEDS.len());
    assert_eq!(integration.schema_version, OUTCOME_SOURCE_SCHEMA_VERSION);
    assert_eq!(integration.waypoints.len(), MISSION_SEEDS.len());
    assert_eq!(
        serde_json::from_slice::<MissionBundle>(
            &serde_json::to_vec(&documentation.bundle).unwrap()
        )
        .unwrap(),
        documentation.bundle
    );
    assert_eq!(
        serde_json::from_slice::<MissionBundle>(&serde_json::to_vec(&ticket.bundle).unwrap())
            .unwrap(),
        ticket.bundle
    );
    assert_eq!(
        serde_json::from_slice::<TypedDraft>(&serde_json::to_vec(&integration).unwrap()).unwrap(),
        integration
    );
    for waypoint in documentation
        .bundle
        .waypoints
        .iter()
        .chain(&ticket.bundle.waypoints)
    {
        assert!(waypoint.outcome_plan.as_ref().is_some_and(|plan| {
            !plan.expected_outcome.trim().is_empty()
                && !plan.alternative_outcomes.is_empty()
                && plan.alternative_outcomes.iter().all(|alternative| {
                    !alternative.outcome.trim().is_empty()
                        && !alternative.acceptable_handling.is_empty()
                })
        }));
    }
    assert!(integration.waypoints.iter().all(|waypoint| {
        waypoint.outcome_plan.as_ref().is_some_and(|plan| {
            !plan.expected_outcome.trim().is_empty()
                && !plan.alternative_outcomes.is_empty()
                && plan.alternative_outcomes.iter().all(|alternative| {
                    !alternative.outcome.trim().is_empty()
                        && !alternative.acceptable_handling.is_empty()
                })
        })
    }));

    let completed = &ticket.bundle.waypoints[15];
    let pending = &ticket.bundle.waypoints[16];
    assert_eq!(completed.state, Some(WaypointState::Blocked));
    assert_eq!(pending.state, Some(WaypointState::Pending));
    assert!(
        ticket
            .bundle
            .waypoints
            .iter()
            .any(|waypoint| { waypoint.ticket_ref.is_none() && waypoint.relationships.is_empty() })
    );
    assert!(
        documentation
            .bundle
            .manifest
            .operator_results
            .iter()
            .any(|result| result.waypoint_id == "code-map")
    );
    for (slug, expected_parts) in [
        ("documentation-workflow", 4),
        ("mission-integration", 9),
        ("ticket-reliability", 17),
    ] {
        let waypoints = corpus
            .join("generated")
            .join(slug)
            .join("generated/waypoints");
        assert!(waypoints.join("README.md").is_file(), "{slug}");
        let part_count = fs::read_dir(waypoints)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some("md")
                    && entry.file_name() != "README.md"
            })
            .count();
        assert_eq!(part_count, expected_parts, "{slug}");
    }
}

#[test]
fn revision_history_retains_changed_request_continuations_and_synthetic_reaction() {
    let workspace = tempfile::Builder::new()
        .prefix(".representative-w8-history-")
        .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    let corpus = fixture_paths();
    let mission_id = generate_fixture(
        workspace.path(),
        &corpus.join("authoring/mission-integration.typed-v3.json"),
    );
    let mission = generated_directory(workspace.path(), mission_id);
    let revision_two = mission.join("history/states/revision-2");
    let current = mission;

    assert_eq!(
        fs::read_to_string(revision_two.join("REQUEST.md")).unwrap(),
        "Synthetic REQUEST state 2."
    );
    assert_eq!(
        fs::read_to_string(current.join("REQUEST.md")).unwrap(),
        "Changed synthetic REQUEST state 3."
    );
    assert!(revision_two.join("inputs/reviewer/initial.md").is_file());
    assert!(current.join("inputs/reviewer/initial.md").is_file());
    assert!(current.join("inputs/reviewer/follow-up.md").is_file());
    let event: MissionApprovalEvent =
        serde_json::from_slice(&fs::read(current.join("history/events/revision-1.json")).unwrap())
            .unwrap();
    assert_eq!(event.reaction.decision, ApprovalDecision::ChangesRequested);
    assert_eq!(
        event.event_id,
        Uuid::from_u128(0x0000000000004000800000000000f00d)
    );
    assert_eq!(event.reaction.actor_source, "fixture-authored");
    assert!(
        event
            .reaction
            .metadata
            .values()
            .any(|value| value.contains("not authentication"))
    );

    let store = MissionStore::open(workspace.path()).with_test_event_metadata(
        Uuid::from_u128(0x0000000000004000800000000000f00d),
        1_798_000_000_000,
    );
    store.verify_output_history(mission_id).unwrap();
    let contract = store.output_contract(mission_id).unwrap();
    assert_eq!(contract.revision, 3);
    assert!(contract.files.iter().any(|file| file.path == "REQUEST.md"));
    assert!(
        contract
            .files
            .iter()
            .any(|file| file.path == "inputs/reviewer/initial.md")
    );
}

#[test]
fn invalid_reference_drift_and_malformed_plan_preserve_accepted_files() {
    let workspace = tempfile::Builder::new()
        .prefix(".representative-w8-negative-")
        .tempdir_in(Path::new(env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    let corpus = fixture_paths();
    let authoring = corpus.join("authoring/documentation-workflow.native-v2.json");
    let input: NativeFixtureInput = serde_json::from_slice(&fs::read(&authoring).unwrap()).unwrap();
    let mission_id = generate_fixture(workspace.path(), &authoring);
    let store = MissionStore::open(workspace.path());
    let directory = generated_directory(workspace.path(), mission_id);

    let before = tree(&directory);
    let mut missing_reference = store.get(mission_id).unwrap().bundle;
    missing_reference.manifest.revision += 1;
    let linked = missing_reference
        .waypoints
        .iter_mut()
        .find(|waypoint| !waypoint.relationships.is_empty())
        .unwrap();
    linked.relationships.specification_ids[0] = Uuid::from_u128(0x0000000000004000800000000000dead);
    let error = store.publish(mission_id, missing_reference, 1).unwrap_err();
    assert_diagnostic(error, "mission-waypoint-link-unavailable");
    assert_eq!(
        tree(&directory),
        before,
        "missing reference changed accepted bytes"
    );

    let generated = directory.join("generated/README.md");
    let mut drifted = fs::read(&generated).unwrap();
    drifted.extend_from_slice(b"\nsynthetic drift\n");
    fs::write(&generated, &drifted).unwrap();
    let drifted_tree = tree(&directory);
    let error = store.verify_output_history(mission_id).unwrap_err();
    assert_diagnostic(error, "mission-output-history-invalid");
    assert_eq!(
        tree(&directory),
        drifted_tree,
        "drift validation rewrote files"
    );
    fs::write(&generated, before.get("generated/README.md").unwrap()).unwrap();

    let mut malformed = input.bundle;
    malformed.manifest.revision = 2;
    malformed.waypoints[0]
        .outcome_plan
        .as_mut()
        .unwrap()
        .expected_outcome
        .clear();
    let before_rejection = tree(&directory);
    let error = store.publish(mission_id, malformed, 1).unwrap_err();
    assert_diagnostic(error, "invalid-outcome-plan");
    assert_eq!(tree(&directory), before_rejection);
}
