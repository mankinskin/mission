# W8. Repair move MCP workspace fixtures (`move-fixtures`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [07-board-fixtures.md](07-board-fixtures.md) | Next: [09-index-output.md](09-index-output.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Exercise same-root, distinct-root, and stale-report fixtures.`

## Part

## Adapted objective

Align move preflight/apply tests with the configured source and destination stores.

## Constraints and boundary

Keep move operations isolated and verify rollback on rejection.

## Validation and non-goal

Validation guidance: Exercise same-root, distinct-root, and stale-report fixtures.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not perform a live store move..

## Fixture-authored outcome guidance

Expected: Each fixture proves the intended source/destination pair and preserves unrelated bytes.

Alternative: The report's source identity no longer matches the fixture root.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Each fixture proves the intended source/destination pair and preserves unrelated bytes.

**Alternative outcome 1:** The report's source identity no longer matches the fixture root.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

