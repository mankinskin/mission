# W5. Resolve descendant Ticket references (`descendant-refs`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [04-missing-prefix.md](04-missing-prefix.md) | Next: [06-default-workspace.md](06-default-workspace.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Test valid descendant references, absent paths, and traversal attempts.`

## Part

## Adapted objective

Allow a selected workspace to follow valid Ticket references into owned descendants.

## Constraints and boundary

Validate canonical IDs and remain within the explicitly selected store.

## Validation and non-goal

Validation guidance: Test valid descendant references, absent paths, and traversal attempts.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not accept arbitrary external stores..

## Fixture-authored outcome guidance

Expected: Valid local references resolve and invalid paths fail before writes.

Alternative: A symlinked descendant escapes the canonical root.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Valid local references resolve and invalid paths fail before writes.

**Alternative outcome 1:** A symlinked descendant escapes the canonical root.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

