# W11. Repair the full Ticket test gate (`test-gate`)

<!-- mission: 00000000-0000-4000-8000-000000003001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [10-update-fixture.md](10-update-fixture.md) | Next: [12-part-layout.md](12-part-layout.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | completed |
| **Prerequisites** | update-fixture |
| **Session package** | w8-test-gate |
| **Prompt** | Fixture-only adaptation: Correct persisted test fixtures to valid schema states so the full package gate reaches the behaviors under test. Preserve the boundary: Normalize only invalid fixture setup; retain legacy values only where a test explicitly covers scan compatibility. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not weaken production state validation or change transport behavior. |
| **Commit checkpoint** | Fixture review checkpoint: test-gate |

## Validation

- `Run the full Ticket package and formatting checks after focused fixture corrections.`

## Part

## Adapted objective

Correct persisted test fixtures to valid schema states so the full package gate reaches the behaviors under test.

## Constraints and boundary

Normalize only invalid fixture setup; retain legacy values only where a test explicitly covers scan compatibility.

## Validation and non-goal

Validation guidance: Run the full Ticket package and formatting checks after focused fixture corrections.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not weaken production state validation or change transport behavior..

## Fixture-authored outcome guidance

Expected: The complete package suite passes against schema-valid state fixtures.

Alternative: A remaining failure is outside invalid fixture setup and requires separate scope.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** The complete package suite passes against schema-valid state fixtures.

**Alternative outcome 1:** A remaining failure is outside invalid fixture setup and requires separate scope.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

