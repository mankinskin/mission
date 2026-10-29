# W16. Apply only separately approved operations (`approved-apply`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [15-root-preflight.md](15-root-preflight.md) | Next: [17-transport-docs.md](17-transport-docs.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Before any future apply, require fresh reports, operation-by-operation approval, and immediate read-back.`

## Part

## Adapted objective

Keep live migration blocked until root ownership is repaired and the exact operation set receives separate approval.

## Constraints and boundary

No approval or ownership may be inferred from clean candidate reports.

## Validation and non-goal

Validation guidance: Before any future apply, require fresh reports, operation-by-operation approval, and immediate read-back.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not apply, verify, or undo any live migration in this fixture..

## Fixture-authored outcome guidance

Expected: The synthetic example remains blocked with no source or store mutation.

Alternative: No separate approval exists for the exact operation order.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The synthetic example remains blocked with no source or store mutation.

**Alternative outcome 1:** No separate approval exists for the exact operation order.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

