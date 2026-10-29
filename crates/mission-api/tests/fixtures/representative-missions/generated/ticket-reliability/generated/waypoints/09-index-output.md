# W9. Align store-index output fixtures (`index-output`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [08-move-fixtures.md](08-move-fixtures.md) | Next: [10-update-fixture.md](10-update-fixture.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | move-fixtures |
| **Session package** | w8-index-output |
| **Prompt** | Fixture-only adaptation: Place generated index artifacts beneath the canonical Ticket store paths used by tests. Preserve the boundary: Test paths must match \`.workflow-tools/ticket\` and stay inside their fixture root. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not change the index contract to fit an incorrect fixture. |
| **Commit checkpoint** | Fixture review checkpoint: index-output |

## Validation

- `Compare exact generated artifact paths and rerun the focused index suite.`

## Part

## Adapted objective

Place generated index artifacts beneath the canonical Ticket store paths used by tests.

## Constraints and boundary

Test paths must match `.workflow-tools/ticket` and stay inside their fixture root.

## Validation and non-goal

Validation guidance: Compare exact generated artifact paths and rerun the focused index suite.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not change the index contract to fit an incorrect fixture..

## Fixture-authored outcome guidance

Expected: The test observes the canonical artifact location and stable contents.

Alternative: An output is created outside the expected store root.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The test observes the canonical artifact location and stable contents.

**Alternative outcome 1:** An output is created outside the expected store root.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

