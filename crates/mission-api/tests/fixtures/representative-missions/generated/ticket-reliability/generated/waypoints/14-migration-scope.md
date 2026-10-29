# W14. Bound migration scope to selected stores (`migration-scope`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [13-cli-dispatch.md](13-cli-dispatch.md) | Next: [15-root-preflight.md](15-root-preflight.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | cli-dispatch |
| **Session package** | w8-migration-scope |
| **Prompt** | Fixture-only adaptation: Compare candidates against every current manifest in the two named stores and no third store. Preserve the boundary: Treat each candidate's provenance separately; exclude legacy flat-content conversion. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not clean unrelated stores or infer common ownership for zero-byte files. |
| **Commit checkpoint** | Fixture review checkpoint: migration-scope |

## Validation

- `Verify root and nested inventories, target paths, and zero-conflict results.`

## Part

## Adapted objective

Compare candidates against every current manifest in the two named stores and no third store.

## Constraints and boundary

Treat each candidate's provenance separately; exclude legacy flat-content conversion.

## Validation and non-goal

Validation guidance: Verify root and nested inventories, target paths, and zero-conflict results.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not clean unrelated stores or infer common ownership for zero-byte files..

## Fixture-authored outcome guidance

Expected: The exact two-store scope is explicit and reproducible.

Alternative: The inventory includes a third root or an ineligible record.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The exact two-store scope is explicit and reproducible.

**Alternative outcome 1:** The inventory includes a third root or an ineligible record.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

