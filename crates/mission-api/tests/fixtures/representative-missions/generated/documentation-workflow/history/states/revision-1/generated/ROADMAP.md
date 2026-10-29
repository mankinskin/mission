# Synthetic documentation workflow fixture

<!-- mission: 00000000-0000-4000-8000-000000001001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

## Objective

Synthetic, read-only adaptation of 4 waypoints. No source roadmap is executed or mutated.

## Notes

- Synthetic adaptation. Mission API predecessor checkpoints encode rendered order and do not assert source-roadmap prerequisites.
- Fixture-authored outcome plans and synthetic local evidence are not requester approvals or real execution evidence.


## Operator Results

| Waypoint | Outcome | Operator command | Evidence |
|---|---|---|---|
| code-map | passed | fixture-authored observation; source workspace was not scanned | ce://default/test/doc-comment-synthetic-check |

## Roadmap Waypoints

### W1. Define the doc-comment contract (`doc-contract`)

| Field | Value |
|---|---|
| **Part** | [waypoints/01-doc-contract.md](waypoints/01-doc-contract.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | None |
| **Session package** | w8-doc-contract |
| **Prompt** | Fixture-only adaptation: Specify concise purpose, behavior or responsibility, intent, relationships, and constraints for each discoverable Rust item kind. Preserve the boundary: Distinguish useful explanation from implementation restatement; define structure, length, unsupported kinds, and justified exceptions. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not document the whole workspace or refactor source. |
| **Commit checkpoint** | Fixture review checkpoint: doc-contract |

## W1 Validation

- `Review representative functions, types, constants, traits, implementations, and modules without inventing behavior.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** A reviewed schema states required content and safe omission cases for each item kind.

**Alternative outcome 1:** A trait method appears redundant but its behavior or exception cannot be verified.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W2. Build the documentation health check (`health-check`)

| Field | Value |
|---|---|
| **Part** | [waypoints/02-health-check.md](waypoints/02-health-check.md) |
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

## W2 Validation

- `Run compliant, missing, overlong, malformed, vague-purpose, and justified-exception fixtures.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Every fixture item receives a status and actionable rule-specific finding.

**Alternative outcome 1:** A parser cannot classify one item kind reliably.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W3. Extract maps from verified rustdoc data (`code-map`)

| Field | Value |
|---|---|
| **Part** | [waypoints/03-code-map.md](waypoints/03-code-map.md) |
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

## W3 Validation

- `Run Cargo doc for representative crates and compare names, locations, descriptions, signatures, and gap reports.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Maps preserve identity and disclose unavailable fields rather than guessing.

**Alternative outcome 1:** Rustdoc JSON omits a signature field required by a consumer.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

### W4. Review semantic duplicate candidates (`semantic-review`)

| Field | Value |
|---|---|
| **Part** | [waypoints/04-semantic-review.md](waypoints/04-semantic-review.md) |
| **Mode** | "execution-side-effect" |
| **Scope** | Ticket-backed: \`ce://default/ticket/00000000-0000-4000-8000-000000001002\` |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | code-map |
| **Session package** | w8-semantic-review |
| **Prompt** | Fixture-only adaptation: Compare normalized documented intent to find likely redundant behavior and missing abstraction opportunities. Preserve the boundary: Similarity narrows the search; evidence must retain element links and candidates remain distinct from confirmed redundancy. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not automatically merge, delete, generalize, or mutate implementations. |
| **Commit checkpoint** | Fixture review checkpoint: semantic-review |

## W4 Validation

- `Compare exact duplicates, related-but-distinct items, and overlapping vocabulary with unrelated behavior.`

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Reports expose evidence-backed candidates for a human-reviewed refactoring decision.

**Alternative outcome 1:** Similar descriptions hide a meaningful behavioral difference.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Linked entities: `semantic-review`

Owning workspace: selected workspace root (local records in .workflow-tools/).

## Ticket: Synthetic representative Mission Ticket

Identity: ce://default/ticket/00000000-0000-4000-8000-000000001002

Ticket state (read-only): done

Execution doc-comment-synthetic-check (Test store: .workflow-tools/test): Passed; validation spec: representative-mission-synthetic-check; detail: Synthetic fixture evidence; no product validation is claimed.

## Specification: Synthetic representative Mission specification

Identity: ce://default/spec/00000000-0000-4000-8000-000000001003

Specification body (read-only): Synthetic local fixture record. Not a requester-approved specification.

### Criterion: doc-comment-review-contract

Specification: [Spec 00000000-0000-4000-8000-000000001003](ce://default/spec/00000000-0000-4000-8000-000000001003); criterion: doc-comment-review-contract

Synthetic criterion doc-comment-review-contract for isolated fixture review.

