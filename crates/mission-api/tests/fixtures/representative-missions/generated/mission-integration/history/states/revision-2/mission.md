# Synthetic Mission integration fixture

Synthetic adaptation of the full Mission integration roadmap; no source work is executed or mutated. Revision 2 preserves the source scope while adding synthetic fixture context.

## Define the mission-domain contract

## Adapted objective

Specify the UUID manifest, objective, roadmap, waypoints, linked domain records, session attachments, store, mutation/read-back invariants, and migration.

## Constraints and validation

Keep source dossiers authoritative until selected migration; preserve the domain crate boundary and exclude scheduling, HTTP, and viewer scope. Validation guidance: Read back the canonical specification and verify ownership, persistence, references, migration, and non-scheduling acceptance criteria.. Non-goal: Do not implement crates, transports, migration code, or scheduling in the specification waypoint..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** One authoritative contract names each record owner and keeps sessions and foreign entities reference-only.

**Alternative outcome 1:** A proposed entity reference crosses a store boundary the contract does not define.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Prepare bounded implementation Tickets

## Adapted objective

Create exactly one Ticket for each ticket-backed API, facade, session-attachment, and migration surface.

## Constraints and validation

Keep W3 alignment, W8 guidance, and W9 closure single-session; preserve status authority in each linked Ticket and reconcile the serial prerequisite chain. Validation guidance: Read each exact Ticket ID, objective, acceptance, Spec link, and dependency edge.. Non-goal: Do not activate implementation before all plan references are read back or create a scheduling Ticket..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** W4-W7 each have one linked bounded Ticket; non-ticket work remains untracked by replacement entities.

**Alternative outcome 1:** A Ticket objective, acceptance, or stored edge differs from the reviewed scope.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Settle the structured roadmap contract

## Adapted objective

Define native and typed JSON, stable IDs, explicit serial order, actual dependency edges, revision-local labels, diagnostics, rendering, and recovery.

## Constraints and validation

Do not infer source prerequisites from presentation order, weaken identity checks, or publish a schema change without approval. Validation guidance: Round-trip representative JSON and compare identity, order, dependencies, diagnostics, versioning, and renderer output.. Non-goal: Do not migrate dossiers or implement an unapproved schema..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The contract separates required identity/order from authored prerequisites and defines versioned publication recovery.

**Alternative outcome 1:** A malformed stable ID, ambiguous edge, or unsupported revision is discovered.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Implement durable Mission persistence

## Adapted objective

Implement UUID-backed validation, import, revision, archive, provenance, relationship resolution, and complete generated output in the internal API.

## Constraints and validation

Use `.workflow-tools/mission/`; preserve typed Parts and original source records; keep transports out of the internal crate. Validation guidance: Run focused persistence, validation, revision, exact renderer, history, and rejected-write-preservation tests.. Non-goal: Do not expose transports or mutate source dossiers..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Rejected writes preserve complete accepted bytes and every published revision remains self-contained and readable.

**Alternative outcome 1:** A revision conflicts with current accepted authority or an external reference is unavailable.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Expose the Mission facade

## Adapted objective

Provide structured-roadmap validate/import/render/publish/check-generated operations over one Mission API through CLI and MCP.

## Constraints and validation

Preserve explicit workspace selection, preview-versus-mutation boundaries, diagnostic parity, and recovery behavior. Validation guidance: Build CLI and MCP and compare representative responses, output destinations, and no-write previews.. Non-goal: Do not add HTTP, viewer, session attachment, or scheduling..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Both transports use shared validation and the selected generated directory without bypasses.

**Alternative outcome 1:** A transport cannot preserve a required diagnostic or mutation boundary.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Attach sessions to Missions

## Adapted objective

Pin and resolve canonical mission URNs across sessions without embedding the Mission or moving the session-local workflow graph.

## Constraints and validation

Attachment is reference metadata only, not scheduling, shared graph ownership, or permission to mutate a Mission. Validation guidance: Exercise valid pins, invalid URNs, and two sessions referencing one Mission with isolated stores.. Non-goal: Do not move `session_workflow_*` APIs or dispatch agents..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Exact references resolve while each session retains its independent local graph.

**Alternative outcome 1:** A referenced Mission is missing, malformed, or selected from the wrong workspace.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Migrate selected dossiers safely

## Adapted objective

Enumerate source folders at runtime, import only the current roadmap, preserve provenance and current links, and report migrated/skipped/blocked inputs.

## Constraints and validation

Leave original dossiers and version snapshots unchanged; derive IDs only by the approved title rule and defer unsupported legacy cases explicitly. Validation guidance: Compare dry-run inventory to filesystem contents, inspect the migrated dossier record, and verify all excluded snapshot paths.. Non-goal: Do not delete source dossiers, impose historical folder counts, or convert every legacy folder..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Eligible current roadmaps are represented while blocked folders and excluded history remain visible and auditable.

**Alternative outcome 1:** A source folder is ambiguous, unsafe, or outside the approved migration boundary.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Retarget authorized guidance

## Adapted objective

Replace only approved dossier routes with durable Mission operations after migration is validated.

## Constraints and validation

Require explicit requester authorization for takeover regardless of prior session status; preserve prior/new owner provenance and avoid unrelated policy edits. Validation guidance: Search only owned guidance for stale routes and verify every changed link and takeover boundary.. Non-goal: Do not rewrite unrelated instructions or infer authority from expiry, pins, or history..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Guidance routes to Mission workflows without weakening the explicit takeover gate.

**Alternative outcome 1:** A guidance owner, target, or takeover authorization cannot be verified.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Close with evidence and dispositions

## Adapted objective

Read back the specification, implementation Tickets, validation executions, migration outcome, and required requester dispositions.

## Constraints and validation

A green test is not review or requester acceptance; record the approved blocked backlog separately from unresolved exceptions. Validation guidance: Verify evidence links, exact entity status, generated output, and closure disposition before the final lifecycle transition.. Non-goal: Do not close with missing gates, unresolved feedback, or unapproved scope..

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Closure follows linked evidence and explicit dispositions, not a summary claim.

**Alternative outcome 1:** A blocked item has no exact requester-approved disposition or validation read-back.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

