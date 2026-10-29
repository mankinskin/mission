# W7. Repair board MCP workspace fixtures (`board-fixtures`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [06-default-workspace.md](06-default-workspace.md) | Next: [08-move-fixtures.md](08-move-fixtures.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Run the focused board MCP suite with exact root read-back.`

## Part

## Adapted objective

Make board tests construct data under the workspace root actually resolved by the server.

## Constraints and boundary

Fix fixtures rather than changing valid production lookup behavior.

## Validation and non-goal

Validation guidance: Run the focused board MCP suite with exact root read-back.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not relax workspace scoping or touch live board state..

## Fixture-authored outcome guidance

Expected: Tests read their own generated board records from the selected root.

Alternative: A test unexpectedly discovers a host-level board store.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Tests read their own generated board records from the selected root.

**Alternative outcome 1:** A test unexpectedly discovers a host-level board store.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

