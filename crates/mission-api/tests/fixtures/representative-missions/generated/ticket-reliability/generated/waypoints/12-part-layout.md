# W12. Scope part-layout migration (`part-layout`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [11-test-gate.md](11-test-gate.md) | Next: [13-cli-dispatch.md](13-cli-dispatch.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | test-gate |
| **Session package** | w8-part-layout |
| **Prompt** | Fixture-only adaptation: Select only existing manifest-referenced part files eligible for canonical naming or cleanup. Preserve the boundary: Never synthesize parts from flat legacy fields; retain stable IDs and order. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not rewrite unreferenced files or migrate outside the chosen stores. |
| **Commit checkpoint** | Fixture review checkpoint: part-layout |

## Validation

- `Test duplicate kinds, frozen empty parts, missing files, and untouched legacy content.`

## Part

## Adapted objective

Select only existing manifest-referenced part files eligible for canonical naming or cleanup.

## Constraints and boundary

Never synthesize parts from flat legacy fields; retain stable IDs and order.

## Validation and non-goal

Validation guidance: Test duplicate kinds, frozen empty parts, missing files, and untouched legacy content.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not rewrite unreferenced files or migrate outside the chosen stores..

## Fixture-authored outcome guidance

Expected: The candidate set is deterministic and every excluded file remains byte-identical.

Alternative: A candidate has ambiguous provenance or no manifest reference.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The candidate set is deterministic and every excluded file remains byte-identical.

**Alternative outcome 1:** A candidate has ambiguous provenance or no manifest reference.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

