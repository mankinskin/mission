# W1. Establish the part-write contract (`part-write`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Next: [02-recoverable-publication.md](02-recoverable-publication.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Run focused Ticket API validation and inspect empty, whitespace, and valid part cases.`

## Part

## Adapted objective

Reject blank Ticket-part content and preserve settled lifecycle constraints for writes.

## Constraints and boundary

Keep the contract narrow; transaction recovery and migration are separate work.

## Validation and non-goal

Validation guidance: Run focused Ticket API validation and inspect empty, whitespace, and valid part cases.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not change unrelated lifecycle states..

## Fixture-authored outcome guidance

Expected: Blank writes fail while valid part content and state rules remain stable.

Alternative: A whitespace-only payload reaches a transport path not covered by the first check.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Blank writes fail while valid part content and state rules remain stable.

**Alternative outcome 1:** A whitespace-only payload reaches a transport path not covered by the first check.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

