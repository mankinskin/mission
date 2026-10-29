# W3. Extract maps from verified rustdoc data (`code-map`)

<!-- mission: 00000000-0000-4000-8000-000000001001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [02-health-check.md](02-health-check.md) | Next: [04-semantic-review.md](04-semantic-review.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | health-check |
| **Session package** | w8-code-map |
| **Prompt** | Fixture-only adaptation: Generate normalized records and per-module maps plus an aggregate index of names, paths, descriptions, and signatures. Preserve the boundary: Evaluate rustdoc JSON first; preserve module identity and report unsupported items and extraction gaps explicitly. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not replace source code, infer missing semantics from HTML, or perform duplicate analysis here. |
| **Commit checkpoint** | Fixture review checkpoint: code-map |

## Validation

- `Run Cargo doc for representative crates and compare names, locations, descriptions, signatures, and gap reports.`

## Part

## Adapted objective

Generate normalized records and per-module maps plus an aggregate index of names, paths, descriptions, and signatures.

## Constraints and boundary

Evaluate rustdoc JSON first; preserve module identity and report unsupported items and extraction gaps explicitly.

## Validation and non-goal

Validation guidance: Run Cargo doc for representative crates and compare names, locations, descriptions, signatures, and gap reports.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not replace source code, infer missing semantics from HTML, or perform duplicate analysis here..

## Fixture-authored outcome guidance

Expected: Maps preserve identity and disclose unavailable fields rather than guessing.

Alternative: Rustdoc JSON omits a signature field required by a consumer.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Maps preserve identity and disclose unavailable fields rather than guessing.

**Alternative outcome 1:** Rustdoc JSON omits a signature field required by a consumer.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Operator Results

| Waypoint | Outcome | Operator command | Evidence |
|---|---|---|---|
| code-map | passed | fixture-authored observation; source workspace was not scanned | ce://default/test/doc-comment-synthetic-check |

