# Synthetic Ticket reliability fixture

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

## Objective

Synthetic, read-only adaptation of 17 waypoints. No source roadmap is executed or mutated.

## Notes

- Synthetic adaptation. Mission API predecessor checkpoints encode rendered order and do not assert source-roadmap prerequisites.
- Fixture-authored outcome plans and synthetic local evidence are not requester approvals or real execution evidence.

## Roadmap Waypoints

### W1. Establish the part-write contract (`part-write`)

| Field | Value |
|---|---|
| **Part** | [waypoints/01-part-write.md](waypoints/01-part-write.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | None |
| **Session package** | w8-part-write |
| **Prompt** | Fixture-only adaptation: Reject blank Ticket-part content and preserve settled lifecycle constraints for writes. Preserve the boundary: Keep the contract narrow; transaction recovery and migration are separate work. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not change unrelated lifecycle states. |
| **Commit checkpoint** | Fixture review checkpoint: part-write |

## W1 Validation

- `Run focused Ticket API validation and inspect empty, whitespace, and valid part cases.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Blank writes fail while valid part content and state rules remain stable.

**Alternative outcome 1:** A whitespace-only payload reaches a transport path not covered by the first check.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W2. Make filesystem publication recoverable (`recoverable-publication`)

| Field | Value |
|---|---|
| **Part** | [waypoints/02-recoverable-publication.md](waypoints/02-recoverable-publication.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | part-write |
| **Session package** | w8-recoverable-publication |
| **Prompt** | Fixture-only adaptation: Stage part bytes and manifest updates with a recoverable transaction and explicit rollback state. Preserve the boundary: A multi-file operation needs a journal; a single rename is not the transaction. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not add migration selection or broad cleanup. |
| **Commit checkpoint** | Fixture review checkpoint: recoverable-publication |

## W2 Validation

- `Inject failures around stage, backup, replace, and recovery; verify old or new complete state.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Every interrupted publication is recoverable without a mixed manifest/file state.

**Alternative outcome 1:** A recovery journal indicates a partial operation whose inputs need inspection.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W3. Decide residual package failures (`residual-test-scope`)

| Field | Value |
|---|---|
| **Part** | [waypoints/03-residual-test-scope.md](waypoints/03-residual-test-scope.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | recoverable-publication |
| **Session package** | w8-residual-test-scope |
| **Prompt** | Fixture-only adaptation: Separate missing server behavior from fixture defects before changing the full package test gate. Preserve the boundary: Keep the gate intact and do not create Mission entities for this Ticket task. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not implement server behavior or weaken the package gate in this decision step. |
| **Commit checkpoint** | Fixture review checkpoint: residual-test-scope |

## W3 Validation

- `Reproduce each failure and record the scoped decision before implementation.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Each residual failure has an evidence-backed owner and an explicit disposition.

**Alternative outcome 1:** A failure cannot be attributed safely from the current fixture evidence.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W4. Report missing-prefix search scope (`missing-prefix`)

| Field | Value |
|---|---|
| **Part** | [waypoints/04-missing-prefix.md](waypoints/04-missing-prefix.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | residual-test-scope |
| **Session package** | w8-missing-prefix |
| **Prompt** | Fixture-only adaptation: Return useful searched-workspace diagnostics when a Ticket identifier prefix matches nothing. Preserve the boundary: Diagnostics are read-only and must reveal no unrelated Ticket content. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not broaden the resolver to arbitrary ancestor stores. |
| **Commit checkpoint** | Fixture review checkpoint: missing-prefix |

## W4 Validation

- `Exercise absent and ambiguous prefixes under explicit workspace roots.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The result names the search scope and leaves every store byte unchanged.

**Alternative outcome 1:** A prefix matches a candidate outside the selected workspace.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W5. Resolve descendant Ticket references (`descendant-refs`)

| Field | Value |
|---|---|
| **Part** | [waypoints/05-descendant-refs.md](waypoints/05-descendant-refs.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | missing-prefix |
| **Session package** | w8-descendant-refs |
| **Prompt** | Fixture-only adaptation: Allow a selected workspace to follow valid Ticket references into owned descendants. Preserve the boundary: Validate canonical IDs and remain within the explicitly selected store. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not accept arbitrary external stores. |
| **Commit checkpoint** | Fixture review checkpoint: descendant-refs |

## W5 Validation

- `Test valid descendant references, absent paths, and traversal attempts.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Valid local references resolve and invalid paths fail before writes.

**Alternative outcome 1:** A symlinked descendant escapes the canonical root.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W6. Honor default-workspace reads (`default-workspace`)

| Field | Value |
|---|---|
| **Part** | [waypoints/06-default-workspace.md](waypoints/06-default-workspace.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | descendant-refs |
| **Session package** | w8-default-workspace |
| **Prompt** | Fixture-only adaptation: Permit read-only operations to use the documented workspace default when a caller omits it. Preserve the boundary: Keep writes explicit and prevent omission from selecting an unrelated store. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not change production workspace policy. |
| **Commit checkpoint** | Fixture review checkpoint: default-workspace |

## W6 Validation

- `Compare omitted and explicit read behavior and assert write rejection without root.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Read queries use the same selected default and writes retain explicit-root requirements.

**Alternative outcome 1:** Default resolution is ambiguous between two candidate workspaces.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W7. Repair board MCP workspace fixtures (`board-fixtures`)

| Field | Value |
|---|---|
| **Part** | [waypoints/07-board-fixtures.md](waypoints/07-board-fixtures.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | default-workspace |
| **Session package** | w8-board-fixtures |
| **Prompt** | Fixture-only adaptation: Make board tests construct data under the workspace root actually resolved by the server. Preserve the boundary: Fix fixtures rather than changing valid production lookup behavior. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not relax workspace scoping or touch live board state. |
| **Commit checkpoint** | Fixture review checkpoint: board-fixtures |

## W7 Validation

- `Run the focused board MCP suite with exact root read-back.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Tests read their own generated board records from the selected root.

**Alternative outcome 1:** A test unexpectedly discovers a host-level board store.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W8. Repair move MCP workspace fixtures (`move-fixtures`)

| Field | Value |
|---|---|
| **Part** | [waypoints/08-move-fixtures.md](waypoints/08-move-fixtures.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | board-fixtures |
| **Session package** | w8-move-fixtures |
| **Prompt** | Fixture-only adaptation: Align move preflight/apply tests with the configured source and destination stores. Preserve the boundary: Keep move operations isolated and verify rollback on rejection. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not perform a live store move. |
| **Commit checkpoint** | Fixture review checkpoint: move-fixtures |

## W8 Validation

- `Exercise same-root, distinct-root, and stale-report fixtures.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Each fixture proves the intended source/destination pair and preserves unrelated bytes.

**Alternative outcome 1:** The report's source identity no longer matches the fixture root.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W9. Align store-index output fixtures (`index-output`)

| Field | Value |
|---|---|
| **Part** | [waypoints/09-index-output.md](waypoints/09-index-output.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | move-fixtures |
| **Session package** | w8-index-output |
| **Prompt** | Fixture-only adaptation: Place generated index artifacts beneath the canonical Ticket store paths used by tests. Preserve the boundary: Test paths must match \`.workflow-tools/ticket\` and stay inside their fixture root. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not change the index contract to fit an incorrect fixture. |
| **Commit checkpoint** | Fixture review checkpoint: index-output |

## W9 Validation

- `Compare exact generated artifact paths and rerun the focused index suite.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The test observes the canonical artifact location and stable contents.

**Alternative outcome 1:** An output is created outside the expected store root.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W10. Correct update MCP workspace fixtures (`update-fixture`)

| Field | Value |
|---|---|
| **Part** | [waypoints/10-update-fixture.md](waypoints/10-update-fixture.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | index-output |
| **Session package** | w8-update-fixture |
| **Prompt** | Fixture-only adaptation: Ensure updates target the same explicit fixture workspace used for setup and read-back. Preserve the boundary: Keep mutation scoped to the synthetic Ticket and verify exact identity after update. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not update any real Ticket. |
| **Commit checkpoint** | Fixture review checkpoint: update-fixture |

## W10 Validation

- `Run update MCP tests against a freshly initialized local store.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The returned and persisted Ticket share the intended canonical ID.

**Alternative outcome 1:** Read-back resolves a different workspace than setup.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W11. Repair the full Ticket test gate (`test-gate`)

| Field | Value |
|---|---|
| **Part** | [waypoints/11-test-gate.md](waypoints/11-test-gate.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | update-fixture |
| **Session package** | w8-test-gate |
| **Prompt** | Fixture-only adaptation: Correct persisted test fixtures to valid schema states so the full package gate reaches the behaviors under test. Preserve the boundary: Normalize only invalid fixture setup; retain legacy values only where a test explicitly covers scan compatibility. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not weaken production state validation or change transport behavior. |
| **Commit checkpoint** | Fixture review checkpoint: test-gate |

## W11 Validation

- `Run the full Ticket package and formatting checks after focused fixture corrections.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The complete package suite passes against schema-valid state fixtures.

**Alternative outcome 1:** A remaining failure is outside invalid fixture setup and requires separate scope.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W12. Scope part-layout migration (`part-layout`)

| Field | Value |
|---|---|
| **Part** | [waypoints/12-part-layout.md](waypoints/12-part-layout.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | test-gate |
| **Session package** | w8-part-layout |
| **Prompt** | Fixture-only adaptation: Select only existing manifest-referenced part files eligible for canonical naming or cleanup. Preserve the boundary: Never synthesize parts from flat legacy fields; retain stable IDs and order. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not rewrite unreferenced files or migrate outside the chosen stores. |
| **Commit checkpoint** | Fixture review checkpoint: part-layout |

## W12 Validation

- `Test duplicate kinds, frozen empty parts, missing files, and untouched legacy content.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The candidate set is deterministic and every excluded file remains byte-identical.

**Alternative outcome 1:** A candidate has ambiguous provenance or no manifest reference.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W13. Correct migration CLI dispatch (`cli-dispatch`)

| Field | Value |
|---|---|
| **Part** | [waypoints/13-cli-dispatch.md](waypoints/13-cli-dispatch.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | part-layout |
| **Session package** | w8-cli-dispatch |
| **Prompt** | Fixture-only adaptation: Ensure the migration command reaches the selected store before producing a dry-run report. Preserve the boundary: Preserve the exact workspace and operation mode through argument dispatch. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not apply or repair migration data here. |
| **Commit checkpoint** | Fixture review checkpoint: cli-dispatch |

## W13 Validation

- `Run the focused dispatch test and assert a same-root report.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Dry-run dispatch returns the selected store's report without mutation.

**Alternative outcome 1:** Dispatch resolves a different store root or no handler.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W14. Bound migration scope to selected stores (`migration-scope`)

| Field | Value |
|---|---|
| **Part** | [waypoints/14-migration-scope.md](waypoints/14-migration-scope.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | cli-dispatch |
| **Session package** | w8-migration-scope |
| **Prompt** | Fixture-only adaptation: Compare candidates against every current manifest in the two named stores and no third store. Preserve the boundary: Treat each candidate's provenance separately; exclude legacy flat-content conversion. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not clean unrelated stores or infer common ownership for zero-byte files. |
| **Commit checkpoint** | Fixture review checkpoint: migration-scope |

## W14 Validation

- `Verify root and nested inventories, target paths, and zero-conflict results.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The exact two-store scope is explicit and reproducible.

**Alternative outcome 1:** The inventory includes a third root or an ineligible record.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W15. Run root-store preflight (`root-preflight`)

| Field | Value |
|---|---|
| **Part** | [waypoints/15-root-preflight.md](waypoints/15-root-preflight.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Ticket-backed: \`ce://default/ticket/00000000-0000-4000-8000-000000003002\` |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | migration-scope |
| **Session package** | w8-root-preflight |
| **Prompt** | Fixture-only adaptation: Check freshness, candidate identity, and preservation for the selected root and nested Ticket stores. Preserve the boundary: The root board ownership query fails with OS error 3; retain that uncertainty and do not apply. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not claim ownership or authorize live migration. |
| **Commit checkpoint** | Fixture review checkpoint: root-preflight |

## W15 Validation

- `Verify both reports, same-root candidate paths, and unchanged store bytes.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Preflight evidence is preserved while unresolved root ownership remains a blocker.

**Alternative outcome 1:** The explicit root query still fails or a report is stale.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W16. Apply only separately approved operations (`approved-apply`)

| Field | Value |
|---|---|
| **Part** | [waypoints/16-approved-apply.md](waypoints/16-approved-apply.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | blocked |
| **Prerequisites** | root-preflight |
| **Session package** | w8-approved-apply |
| **Prompt** | Fixture-only adaptation: Keep live migration blocked until root ownership is repaired and the exact operation set receives separate approval. Preserve the boundary: No approval or ownership may be inferred from clean candidate reports. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not apply, verify, or undo any live migration in this fixture. |
| **Commit checkpoint** | Fixture review checkpoint: approved-apply |

## W16 Validation

- `Before any future apply, require fresh reports, operation-by-operation approval, and immediate read-back.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The synthetic example remains blocked with no source or store mutation.

**Alternative outcome 1:** No separate approval exists for the exact operation order.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W17. Integrate transport and settled behavior (`transport-docs`)

| Field | Value |
|---|---|
| **Part** | [waypoints/17-transport-docs.md](waypoints/17-transport-docs.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | approved-apply |
| **Session package** | w8-transport-docs |
| **Prompt** | Fixture-only adaptation: Document validated Ticket-part behavior consistently across CLI, MCP, and owned usage guidance. Preserve the boundary: Retain the two-store boundary and keep evidence/status authorities separate. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not create new entities or broaden migration. |
| **Commit checkpoint** | Fixture review checkpoint: transport-docs |

## W17 Validation

- `Run Ticket API/package and transport checks, then read back each validation record.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Transport behavior and documentation agree with the validated contract.

**Alternative outcome 1:** A transport contract or required validation read-back is missing.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Linked entities: `root-preflight`

Owning workspace: selected workspace root (local records in .workflow-tools/).

## Ticket: Synthetic representative Mission Ticket

Identity: ce://default/ticket/00000000-0000-4000-8000-000000003002

Ticket state (read-only): done

Execution ticket-reliability-synthetic-check (Test store: .workflow-tools/test): Passed; validation spec: representative-mission-synthetic-check; detail: Synthetic fixture evidence; no product validation is claimed.

## Specification: Synthetic representative Mission specification

Identity: ce://default/spec/00000000-0000-4000-8000-000000003003

Specification body (read-only): Synthetic local fixture record. Not a requester-approved specification.

### Criterion: ticket-migration-safety

Specification: [Spec 00000000-0000-4000-8000-000000003003](ce://default/spec/00000000-0000-4000-8000-000000003003); criterion: ticket-migration-safety

Synthetic criterion ticket-migration-safety for isolated fixture review.

