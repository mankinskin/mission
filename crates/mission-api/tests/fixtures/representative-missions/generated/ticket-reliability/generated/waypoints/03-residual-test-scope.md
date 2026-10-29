# W3. Decide residual package failures (`residual-test-scope`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [02-recoverable-publication.md](02-recoverable-publication.md) | Next: [04-missing-prefix.md](04-missing-prefix.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Reproduce each failure and record the scoped decision before implementation.`

## Part

## Adapted objective

Separate missing server behavior from fixture defects before changing the full package test gate.

## Constraints and boundary

Keep the gate intact and do not create Mission entities for this Ticket task.

## Validation and non-goal

Validation guidance: Reproduce each failure and record the scoped decision before implementation.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not implement server behavior or weaken the package gate in this decision step..

## Fixture-authored outcome guidance

Expected: Each residual failure has an evidence-backed owner and an explicit disposition.

Alternative: A failure cannot be attributed safely from the current fixture evidence.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Each residual failure has an evidence-backed owner and an explicit disposition.

**Alternative outcome 1:** A failure cannot be attributed safely from the current fixture evidence.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

