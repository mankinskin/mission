# W2. Make filesystem publication recoverable (`recoverable-publication`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [01-part-write.md](01-part-write.md) | Next: [03-residual-test-scope.md](03-residual-test-scope.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Inject failures around stage, backup, replace, and recovery; verify old or new complete state.`

## Part

## Adapted objective

Stage part bytes and manifest updates with a recoverable transaction and explicit rollback state.

## Constraints and boundary

A multi-file operation needs a journal; a single rename is not the transaction.

## Validation and non-goal

Validation guidance: Inject failures around stage, backup, replace, and recovery; verify old or new complete state.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not add migration selection or broad cleanup..

## Fixture-authored outcome guidance

Expected: Every interrupted publication is recoverable without a mixed manifest/file state.

Alternative: A recovery journal indicates a partial operation whose inputs need inspection.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Every interrupted publication is recoverable without a mixed manifest/file state.

**Alternative outcome 1:** A recovery journal indicates a partial operation whose inputs need inspection.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

