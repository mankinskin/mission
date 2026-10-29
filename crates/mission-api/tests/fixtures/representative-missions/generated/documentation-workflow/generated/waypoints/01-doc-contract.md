# W1. Define the doc-comment contract (`doc-contract`)

<!-- mission: 00000000-0000-4000-8000-000000001001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Next: [02-health-check.md](02-health-check.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | None |
| **Session package** | w8-doc-contract |
| **Prompt** | Fixture-only adaptation: Specify concise purpose, behavior or responsibility, intent, relationships, and constraints for each discoverable Rust item kind. Preserve the boundary: Distinguish useful explanation from implementation restatement; define structure, length, unsupported kinds, and justified exceptions. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not document the whole workspace or refactor source. |
| **Commit checkpoint** | Fixture review checkpoint: doc-contract |

## Validation

- `Review representative functions, types, constants, traits, implementations, and modules without inventing behavior.`

## Part

## Adapted objective

Specify concise purpose, behavior or responsibility, intent, relationships, and constraints for each discoverable Rust item kind.

## Constraints and boundary

Distinguish useful explanation from implementation restatement; define structure, length, unsupported kinds, and justified exceptions.

## Validation and non-goal

Validation guidance: Review representative functions, types, constants, traits, implementations, and modules without inventing behavior.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not document the whole workspace or refactor source..

## Fixture-authored outcome guidance

Expected: A reviewed schema states required content and safe omission cases for each item kind.

Alternative: A trait method appears redundant but its behavior or exception cannot be verified.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** A reviewed schema states required content and safe omission cases for each item kind.

**Alternative outcome 1:** A trait method appears redundant but its behavior or exception cannot be verified.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

