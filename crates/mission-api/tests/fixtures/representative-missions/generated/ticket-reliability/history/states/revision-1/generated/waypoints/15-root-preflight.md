# W15. Run root-store preflight (`root-preflight`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [14-migration-scope.md](14-migration-scope.md) | Next: [16-approved-apply.md](16-approved-apply.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Ticket-backed: \`ce://default/ticket/00000000-0000-4000-8000-000000003002\` |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | migration-scope |
| **Session package** | w8-root-preflight |
| **Prompt** | Fixture-only adaptation: Check freshness, candidate identity, and preservation for the selected root and nested Ticket stores. Preserve the boundary: The root board ownership query fails with OS error 3; retain that uncertainty and do not apply. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not claim ownership or authorize live migration. |
| **Commit checkpoint** | Fixture review checkpoint: root-preflight |

## Validation

- `Verify both reports, same-root candidate paths, and unchanged store bytes.`

## Part

## Adapted objective

Check freshness, candidate identity, and preservation for the selected root and nested Ticket stores.

## Constraints and boundary

The root board ownership query fails with OS error 3; retain that uncertainty and do not apply.

## Validation and non-goal

Validation guidance: Verify both reports, same-root candidate paths, and unchanged store bytes.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not claim ownership or authorize live migration..

## Fixture-authored outcome guidance

Expected: Preflight evidence is preserved while unresolved root ownership remains a blocker.

Alternative: The explicit root query still fails or a report is stale.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Preflight evidence is preserved while unresolved root ownership remains a blocker.

**Alternative outcome 1:** The explicit root query still fails or a report is stale.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Linked entities

Owning workspace: selected workspace root (local records in .workflow-tools/).

## Ticket: Synthetic representative Mission Ticket

Identity: ce://default/ticket/00000000-0000-4000-8000-000000003002

Ticket state (read-only): done

Execution ticket-reliability-synthetic-check (Test store: .workflow-tools/test): Passed; validation spec: representative-mission-synthetic-check; detail: Synthetic fixture evidence; no product validation is claimed.

## Specification: Synthetic representative Mission specification

Identity: ce://default/spec/00000000-0000-4000-8000-000000003003

Specification body (read-only): Synthetic local fixture record. Not a requester-approved specification.

### Criterion: ticket-migration-safety

Specification: [Spec 00000000-0000-4000-8000-000000003003](ce://default/spec/00000000-0000-4000-8000-000000003003); criterion: ticket-migration-safety

Synthetic criterion ticket-migration-safety for isolated fixture review.

