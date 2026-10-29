# W6. Attach sessions to Missions (`session-attachment`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 3; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [05-mission-transports.md](05-mission-transports.md) | Next: [07-dossier-migration.md](07-dossier-migration.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | mission-transports |
| **Session package** | session-attachment |
| **Prompt** | Fixture-only adaptation: Pin and resolve canonical mission URNs across sessions without embedding the Mission or moving the session-local workflow graph. Preserve the boundary: Attachment is reference metadata only, not scheduling, shared graph ownership, or permission to mutate a Mission. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not move \`session\_workflow\_\*\` APIs or dispatch agents. |
| **Commit checkpoint** | Fixture review checkpoint: session-attachment |

## Validation

- `Exercise valid pins, invalid URNs, and two sessions referencing one Mission with isolated stores.`

## Part

## Adapted objective

Pin and resolve canonical mission URNs across sessions without embedding the Mission or moving the session-local workflow graph.

## Constraints and validation

Attachment is reference metadata only, not scheduling, shared graph ownership, or permission to mutate a Mission. Validation guidance: Exercise valid pins, invalid URNs, and two sessions referencing one Mission with isolated stores.. Non-goal: Do not move `session_workflow_*` APIs or dispatch agents..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Exact references resolve while each session retains its independent local graph.

**Alternative outcome 1:** A referenced Mission is missing, malformed, or selected from the wrong workspace.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

