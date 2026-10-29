# W8. Retarget authorized guidance (`guidance-retarget`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [07-dossier-migration.md](07-dossier-migration.md) | Next: [09-validation-closure.md](09-validation-closure.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | dossier-migration |
| **Session package** | guidance-retarget |
| **Prompt** | Fixture-only adaptation: Replace only approved dossier routes with durable Mission operations after migration is validated. Preserve the boundary: Require explicit requester authorization for takeover regardless of prior session status; preserve prior/new owner provenance and avoid unrelated policy edits. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not rewrite unrelated instructions or infer authority from expiry, pins, or history. |
| **Commit checkpoint** | Fixture review checkpoint: guidance-retarget |

## Validation

- `Search only owned guidance for stale routes and verify every changed link and takeover boundary.`

## Part

## Adapted objective

Replace only approved dossier routes with durable Mission operations after migration is validated.

## Constraints and validation

Require explicit requester authorization for takeover regardless of prior session status; preserve prior/new owner provenance and avoid unrelated policy edits. Validation guidance: Search only owned guidance for stale routes and verify every changed link and takeover boundary.. Non-goal: Do not rewrite unrelated instructions or infer authority from expiry, pins, or history..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Guidance routes to Mission workflows without weakening the explicit takeover gate.

**Alternative outcome 1:** A guidance owner, target, or takeover authorization cannot be verified.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

