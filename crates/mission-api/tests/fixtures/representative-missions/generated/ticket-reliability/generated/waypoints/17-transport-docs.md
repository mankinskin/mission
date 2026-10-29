# W17. Integrate transport and settled behavior (`transport-docs`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [16-approved-apply.md](16-approved-apply.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | approved-apply |
| **Session package** | w8-transport-docs |
| **Prompt** | Fixture-only adaptation: Document validated Ticket-part behavior consistently across CLI, MCP, and owned usage guidance. Preserve the boundary: Retain the two-store boundary and keep evidence/status authorities separate. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not create new entities or broaden migration. |
| **Commit checkpoint** | Fixture review checkpoint: transport-docs |

## Validation

- `Run Ticket API/package and transport checks, then read back each validation record.`

## Part

## Adapted objective

Document validated Ticket-part behavior consistently across CLI, MCP, and owned usage guidance.

## Constraints and boundary

Retain the two-store boundary and keep evidence/status authorities separate.

## Validation and non-goal

Validation guidance: Run Ticket API/package and transport checks, then read back each validation record.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not create new entities or broaden migration..

## Fixture-authored outcome guidance

Expected: Transport behavior and documentation agree with the validated contract.

Alternative: A transport contract or required validation read-back is missing.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Transport behavior and documentation agree with the validated contract.

**Alternative outcome 1:** A transport contract or required validation read-back is missing.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

