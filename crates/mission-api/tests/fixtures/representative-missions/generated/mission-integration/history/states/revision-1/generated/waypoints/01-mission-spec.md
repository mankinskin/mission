# W1. Define the mission-domain contract (`mission-spec`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Next: [02-ticket-set.md](02-ticket-set.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | None |
| **Session package** | mission-spec |
| **Prompt** | Fixture-only adaptation: Specify the UUID manifest, objective, roadmap, waypoints, linked domain records, session attachments, store, mutation/read-back invariants, and migration. Preserve the boundary: Keep source dossiers authoritative until selected migration; preserve the domain crate boundary and exclude scheduling, HTTP, and viewer scope. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not implement crates, transports, migration code, or scheduling in the specification waypoint. |
| **Commit checkpoint** | Fixture review checkpoint: mission-spec |

## Validation

- `Read back the canonical specification and verify ownership, persistence, references, migration, and non-scheduling acceptance criteria.`

## Part

## Adapted objective

Specify the UUID manifest, objective, roadmap, waypoints, linked domain records, session attachments, store, mutation/read-back invariants, and migration.

## Constraints and validation

Keep source dossiers authoritative until selected migration; preserve the domain crate boundary and exclude scheduling, HTTP, and viewer scope. Validation guidance: Read back the canonical specification and verify ownership, persistence, references, migration, and non-scheduling acceptance criteria.. Non-goal: Do not implement crates, transports, migration code, or scheduling in the specification waypoint..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** One authoritative contract names each record owner and keeps sessions and foreign entities reference-only.

**Alternative outcome 1:** A proposed entity reference crosses a store boundary the contract does not define.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

