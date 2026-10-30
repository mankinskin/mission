#[derive(Clone, Copy)]
pub(crate) struct Seed {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) objective: &'static str,
    pub(crate) constraints: &'static str,
    pub(crate) validation: &'static str,
    pub(crate) non_goal: &'static str,
    pub(crate) expected: &'static str,
    pub(crate) alternative: &'static str,
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

pub(crate) const DOC_SEEDS: &[Seed] = &[
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

pub(crate) const MISSION_SEEDS: &[Seed] = &[
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

pub(crate) const TICKET_SEEDS: &[Seed] = &[
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
