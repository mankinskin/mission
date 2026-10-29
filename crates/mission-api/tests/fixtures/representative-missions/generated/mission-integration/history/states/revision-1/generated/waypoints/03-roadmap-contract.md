# W3. Settle the structured roadmap contract (`roadmap-contract`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [02-ticket-set.md](02-ticket-set.md) | Next: [04-mission-api.md](04-mission-api.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | ticket-set |
| **Session package** | roadmap-contract |
| **Prompt** | Fixture-only adaptation: Define native and typed JSON, stable IDs, explicit serial order, actual dependency edges, revision-local labels, diagnostics, rendering, and recovery. Preserve the boundary: Do not infer source prerequisites from presentation order, weaken identity checks, or publish a schema change without approval. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not migrate dossiers or implement an unapproved schema. |
| **Commit checkpoint** | Fixture review checkpoint: roadmap-contract |

## Validation

- `Round-trip representative JSON and compare identity, order, dependencies, diagnostics, versioning, and renderer output.`

## Part

## Adapted objective

Define native and typed JSON, stable IDs, explicit serial order, actual dependency edges, revision-local labels, diagnostics, rendering, and recovery.

## Constraints and validation

Do not infer source prerequisites from presentation order, weaken identity checks, or publish a schema change without approval. Validation guidance: Round-trip representative JSON and compare identity, order, dependencies, diagnostics, versioning, and renderer output.. Non-goal: Do not migrate dossiers or implement an unapproved schema..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The contract separates required identity/order from authored prerequisites and defines versioned publication recovery.

**Alternative outcome 1:** A malformed stable ID, ambiguous edge, or unsupported revision is discovered.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

