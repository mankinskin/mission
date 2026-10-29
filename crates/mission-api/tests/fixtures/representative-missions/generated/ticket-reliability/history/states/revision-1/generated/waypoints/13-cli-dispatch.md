# W13. Correct migration CLI dispatch (`cli-dispatch`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [12-part-layout.md](12-part-layout.md) | Next: [14-migration-scope.md](14-migration-scope.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | part-layout |
| **Session package** | w8-cli-dispatch |
| **Prompt** | Fixture-only adaptation: Ensure the migration command reaches the selected store before producing a dry-run report. Preserve the boundary: Preserve the exact workspace and operation mode through argument dispatch. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not apply or repair migration data here. |
| **Commit checkpoint** | Fixture review checkpoint: cli-dispatch |

## Validation

- `Run the focused dispatch test and assert a same-root report.`

## Part

## Adapted objective

Ensure the migration command reaches the selected store before producing a dry-run report.

## Constraints and boundary

Preserve the exact workspace and operation mode through argument dispatch.

## Validation and non-goal

Validation guidance: Run the focused dispatch test and assert a same-root report.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not apply or repair migration data here..

## Fixture-authored outcome guidance

Expected: Dry-run dispatch returns the selected store's report without mutation.

Alternative: Dispatch resolves a different store root or no handler.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Dry-run dispatch returns the selected store's report without mutation.

**Alternative outcome 1:** Dispatch resolves a different store root or no handler.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

