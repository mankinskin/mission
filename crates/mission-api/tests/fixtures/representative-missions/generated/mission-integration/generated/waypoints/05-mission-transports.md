# W5. Expose the Mission facade (`mission-transports`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 3; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [04-mission-api.md](04-mission-api.md) | Next: [06-session-attachment.md](06-session-attachment.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | mission-api |
| **Session package** | mission-transports |
| **Prompt** | Fixture-only adaptation: Provide structured-roadmap validate/import/render/publish/check-generated operations over one Mission API through CLI and MCP. Preserve the boundary: Preserve explicit workspace selection, preview-versus-mutation boundaries, diagnostic parity, and recovery behavior. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not add HTTP, viewer, session attachment, or scheduling. |
| **Commit checkpoint** | Fixture review checkpoint: mission-transports |

## Validation

- `Build CLI and MCP and compare representative responses, output destinations, and no-write previews.`

## Part

## Adapted objective

Provide structured-roadmap validate/import/render/publish/check-generated operations over one Mission API through CLI and MCP.

## Constraints and validation

Preserve explicit workspace selection, preview-versus-mutation boundaries, diagnostic parity, and recovery behavior. Validation guidance: Build CLI and MCP and compare representative responses, output destinations, and no-write previews.. Non-goal: Do not add HTTP, viewer, session attachment, or scheduling..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Both transports use shared validation and the selected generated directory without bypasses.

**Alternative outcome 1:** A transport cannot preserve a required diagnostic or mutation boundary.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

