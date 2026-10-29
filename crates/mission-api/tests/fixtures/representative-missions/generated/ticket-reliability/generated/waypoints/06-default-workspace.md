# W6. Honor default-workspace reads (`default-workspace`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [05-descendant-refs.md](05-descendant-refs.md) | Next: [07-board-fixtures.md](07-board-fixtures.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Compare omitted and explicit read behavior and assert write rejection without root.`

## Part

## Adapted objective

Permit read-only operations to use the documented workspace default when a caller omits it.

## Constraints and boundary

Keep writes explicit and prevent omission from selecting an unrelated store.

## Validation and non-goal

Validation guidance: Compare omitted and explicit read behavior and assert write rejection without root.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not change production workspace policy..

## Fixture-authored outcome guidance

Expected: Read queries use the same selected default and writes retain explicit-root requirements.

Alternative: Default resolution is ambiguous between two candidate workspaces.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Read queries use the same selected default and writes retain explicit-root requirements.

**Alternative outcome 1:** Default resolution is ambiguous between two candidate workspaces.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

