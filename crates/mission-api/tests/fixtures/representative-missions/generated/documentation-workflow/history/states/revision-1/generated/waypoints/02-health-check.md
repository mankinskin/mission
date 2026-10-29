# W2. Build the documentation health check (`health-check`)

<!-- mission: 00000000-0000-4000-8000-000000001001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [01-doc-contract.md](01-doc-contract.md) | Next: [03-code-map.md](03-code-map.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | doc-contract |
| **Session package** | w8-health-check |
| **Prompt** | Fixture-only adaptation: Inventory selected modules and report missing, malformed, vague, overlong, or unsupported documentation with stable status over time. Preserve the boundary: Every finding identifies the item, location, violated contract rule, and whether the case is intentionally undocumented or unsupported. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not silently rewrite comments or modify Rust source. |
| **Commit checkpoint** | Fixture review checkpoint: health-check |

## Validation

- `Run compliant, missing, overlong, malformed, vague-purpose, and justified-exception fixtures.`

## Part

## Adapted objective

Inventory selected modules and report missing, malformed, vague, overlong, or unsupported documentation with stable status over time.

## Constraints and boundary

Every finding identifies the item, location, violated contract rule, and whether the case is intentionally undocumented or unsupported.

## Validation and non-goal

Validation guidance: Run compliant, missing, overlong, malformed, vague-purpose, and justified-exception fixtures.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not silently rewrite comments or modify Rust source..

## Fixture-authored outcome guidance

Expected: Every fixture item receives a status and actionable rule-specific finding.

Alternative: A parser cannot classify one item kind reliably.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Every fixture item receives a status and actionable rule-specific finding.

**Alternative outcome 1:** A parser cannot classify one item kind reliably.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

