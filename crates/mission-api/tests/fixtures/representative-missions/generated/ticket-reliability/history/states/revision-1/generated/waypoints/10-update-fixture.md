# W10. Correct update MCP workspace fixtures (`update-fixture`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [09-index-output.md](09-index-output.md) | Next: [11-test-gate.md](11-test-gate.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Run update MCP tests against a freshly initialized local store.`

## Part

## Adapted objective

Ensure updates target the same explicit fixture workspace used for setup and read-back.

## Constraints and boundary

Keep mutation scoped to the synthetic Ticket and verify exact identity after update.

## Validation and non-goal

Validation guidance: Run update MCP tests against a freshly initialized local store.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not update any real Ticket..

## Fixture-authored outcome guidance

Expected: The returned and persisted Ticket share the intended canonical ID.

Alternative: Read-back resolves a different workspace than setup.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The returned and persisted Ticket share the intended canonical ID.

**Alternative outcome 1:** Read-back resolves a different workspace than setup.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

