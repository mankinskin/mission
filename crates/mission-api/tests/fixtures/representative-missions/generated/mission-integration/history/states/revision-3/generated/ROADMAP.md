# Synthetic Mission integration fixture

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 3; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

## Objective

Synthetic adaptation of the full Mission integration roadmap; no source work is executed or mutated. Revision 2 preserves the source scope while adding synthetic fixture context. Revision 3 records a changed synthetic request without replacing prior continuation.

## Roadmap Waypoints

### W1. Define the mission-domain contract (`mission-spec`)

| Field | Value |
|---|---|
| **Part** | [waypoints/01-mission-spec.md](waypoints/01-mission-spec.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | None |
| **Session package** | mission-spec |
| **Prompt** | Fixture-only adaptation: Specify the UUID manifest, objective, roadmap, waypoints, linked domain records, session attachments, store, mutation/read-back invariants, and migration. Preserve the boundary: Keep source dossiers authoritative until selected migration; preserve the domain crate boundary and exclude scheduling, HTTP, and viewer scope. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not implement crates, transports, migration code, or scheduling in the specification waypoint. |
| **Commit checkpoint** | Fixture review checkpoint: mission-spec |

## W1 Validation

- `Read back the canonical specification and verify ownership, persistence, references, migration, and non-scheduling acceptance criteria.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** One authoritative contract names each record owner and keeps sessions and foreign entities reference-only.

**Alternative outcome 1:** A proposed entity reference crosses a store boundary the contract does not define.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W2. Prepare bounded implementation Tickets (`ticket-set`)

| Field | Value |
|---|---|
| **Part** | [waypoints/02-ticket-set.md](waypoints/02-ticket-set.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | mission-spec |
| **Session package** | ticket-set |
| **Prompt** | Fixture-only adaptation: Create exactly one Ticket for each ticket-backed API, facade, session-attachment, and migration surface. Preserve the boundary: Keep W3 alignment, W8 guidance, and W9 closure single-session; preserve status authority in each linked Ticket and reconcile the serial prerequisite chain. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not activate implementation before all plan references are read back or create a scheduling Ticket. |
| **Commit checkpoint** | Fixture review checkpoint: ticket-set |

## W2 Validation

- `Read each exact Ticket ID, objective, acceptance, Spec link, and dependency edge.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** W4-W7 each have one linked bounded Ticket; non-ticket work remains untracked by replacement entities.

**Alternative outcome 1:** A Ticket objective, acceptance, or stored edge differs from the reviewed scope.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W3. Settle the structured roadmap contract (`roadmap-contract`)

| Field | Value |
|---|---|
| **Part** | [waypoints/03-roadmap-contract.md](waypoints/03-roadmap-contract.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | ticket-set |
| **Session package** | roadmap-contract |
| **Prompt** | Fixture-only adaptation: Define native and typed JSON, stable IDs, explicit serial order, actual dependency edges, revision-local labels, diagnostics, rendering, and recovery. Preserve the boundary: Do not infer source prerequisites from presentation order, weaken identity checks, or publish a schema change without approval. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not migrate dossiers or implement an unapproved schema. |
| **Commit checkpoint** | Fixture review checkpoint: roadmap-contract |

## W3 Validation

- `Round-trip representative JSON and compare identity, order, dependencies, diagnostics, versioning, and renderer output.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The contract separates required identity/order from authored prerequisites and defines versioned publication recovery.

**Alternative outcome 1:** A malformed stable ID, ambiguous edge, or unsupported revision is discovered.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W4. Implement durable Mission persistence (`mission-api`)

| Field | Value |
|---|---|
| **Part** | [waypoints/04-mission-api.md](waypoints/04-mission-api.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Ticket-backed: \`ce://default/ticket/00000000-0000-4000-8000-000000002002\` |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | roadmap-contract |
| **Session package** | mission-api |
| **Prompt** | Fixture-only adaptation: Implement UUID-backed validation, import, revision, archive, provenance, relationship resolution, and complete generated output in the internal API. Preserve the boundary: Use \`.workflow-tools/mission/\`; preserve typed Parts and original source records; keep transports out of the internal crate. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not expose transports or mutate source dossiers. |
| **Commit checkpoint** | Fixture review checkpoint: mission-api |

## W4 Validation

- `Run focused persistence, validation, revision, exact renderer, history, and rejected-write-preservation tests.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Rejected writes preserve complete accepted bytes and every published revision remains self-contained and readable.

**Alternative outcome 1:** A revision conflicts with current accepted authority or an external reference is unavailable.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W5. Expose the Mission facade (`mission-transports`)

| Field | Value |
|---|---|
| **Part** | [waypoints/05-mission-transports.md](waypoints/05-mission-transports.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | mission-api |
| **Session package** | mission-transports |
| **Prompt** | Fixture-only adaptation: Provide structured-roadmap validate/import/render/publish/check-generated operations over one Mission API through CLI and MCP. Preserve the boundary: Preserve explicit workspace selection, preview-versus-mutation boundaries, diagnostic parity, and recovery behavior. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not add HTTP, viewer, session attachment, or scheduling. |
| **Commit checkpoint** | Fixture review checkpoint: mission-transports |

## W5 Validation

- `Build CLI and MCP and compare representative responses, output destinations, and no-write previews.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Both transports use shared validation and the selected generated directory without bypasses.

**Alternative outcome 1:** A transport cannot preserve a required diagnostic or mutation boundary.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W6. Attach sessions to Missions (`session-attachment`)

| Field | Value |
|---|---|
| **Part** | [waypoints/06-session-attachment.md](waypoints/06-session-attachment.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | mission-transports |
| **Session package** | session-attachment |
| **Prompt** | Fixture-only adaptation: Pin and resolve canonical mission URNs across sessions without embedding the Mission or moving the session-local workflow graph. Preserve the boundary: Attachment is reference metadata only, not scheduling, shared graph ownership, or permission to mutate a Mission. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not move \`session\_workflow\_\*\` APIs or dispatch agents. |
| **Commit checkpoint** | Fixture review checkpoint: session-attachment |

## W6 Validation

- `Exercise valid pins, invalid URNs, and two sessions referencing one Mission with isolated stores.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Exact references resolve while each session retains its independent local graph.

**Alternative outcome 1:** A referenced Mission is missing, malformed, or selected from the wrong workspace.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W7. Migrate selected dossiers safely (`dossier-migration`)

| Field | Value |
|---|---|
| **Part** | [waypoints/07-dossier-migration.md](waypoints/07-dossier-migration.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | session-attachment |
| **Session package** | dossier-migration |
| **Prompt** | Fixture-only adaptation: Enumerate source folders at runtime, import only the current roadmap, preserve provenance and current links, and report migrated/skipped/blocked inputs. Preserve the boundary: Leave original dossiers and version snapshots unchanged; derive IDs only by the approved title rule and defer unsupported legacy cases explicitly. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not delete source dossiers, impose historical folder counts, or convert every legacy folder. |
| **Commit checkpoint** | Fixture review checkpoint: dossier-migration |

## W7 Validation

- `Compare dry-run inventory to filesystem contents, inspect the migrated dossier record, and verify all excluded snapshot paths.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Eligible current roadmaps are represented while blocked folders and excluded history remain visible and auditable.

**Alternative outcome 1:** A source folder is ambiguous, unsafe, or outside the approved migration boundary.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W8. Retarget authorized guidance (`guidance-retarget`)

| Field | Value |
|---|---|
| **Part** | [waypoints/08-guidance-retarget.md](waypoints/08-guidance-retarget.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | dossier-migration |
| **Session package** | guidance-retarget |
| **Prompt** | Fixture-only adaptation: Replace only approved dossier routes with durable Mission operations after migration is validated. Preserve the boundary: Require explicit requester authorization for takeover regardless of prior session status; preserve prior/new owner provenance and avoid unrelated policy edits. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not rewrite unrelated instructions or infer authority from expiry, pins, or history. |
| **Commit checkpoint** | Fixture review checkpoint: guidance-retarget |

## W8 Validation

- `Search only owned guidance for stale routes and verify every changed link and takeover boundary.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Guidance routes to Mission workflows without weakening the explicit takeover gate.

**Alternative outcome 1:** A guidance owner, target, or takeover authorization cannot be verified.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W9. Close with evidence and dispositions (`validation-closure`)

| Field | Value |
|---|---|
| **Part** | [waypoints/09-validation-closure.md](waypoints/09-validation-closure.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | guidance-retarget |
| **Session package** | validation-closure |
| **Prompt** | Fixture-only adaptation: Read back the specification, implementation Tickets, validation executions, migration outcome, and required requester dispositions. Preserve the boundary: A green test is not review or requester acceptance; record the approved blocked backlog separately from unresolved exceptions. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not close with missing gates, unresolved feedback, or unapproved scope. |
| **Commit checkpoint** | Fixture review checkpoint: validation-closure |

## W9 Validation

- `Verify evidence links, exact entity status, generated output, and closure disposition before the final lifecycle transition.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Closure follows linked evidence and explicit dispositions, not a summary claim.

**Alternative outcome 1:** A blocked item has no exact requester-approved disposition or validation read-back.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Linked entities: `mission-api`

Owning workspace: selected workspace root (local records in .workflow-tools/).

## Ticket: Synthetic representative Mission Ticket

Identity: ce://default/ticket/00000000-0000-4000-8000-000000002002

Ticket state (read-only): done

Execution mission-integration-synthetic-check (Test store: .workflow-tools/test): Passed; validation spec: representative-mission-synthetic-check; detail: Synthetic fixture evidence; no product validation is claimed.

## Specification: Synthetic representative Mission specification

Identity: ce://default/spec/00000000-0000-4000-8000-000000002003

Specification body (read-only): Synthetic local fixture record. Not a requester-approved specification.

### Criterion: mission-domain-boundary

Specification: [Spec 00000000-0000-4000-8000-000000002003](ce://default/spec/00000000-0000-4000-8000-000000002003); criterion: mission-domain-boundary

Synthetic criterion mission-domain-boundary for isolated fixture review.

