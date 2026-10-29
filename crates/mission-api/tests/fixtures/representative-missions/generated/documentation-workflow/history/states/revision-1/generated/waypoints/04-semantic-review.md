# W4. Review semantic duplicate candidates (`semantic-review`)

<!-- mission: 00000000-0000-4000-8000-000000001001; revision: 1; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [03-code-map.md](03-code-map.md)

## Waypoint Details

| Field | Value |
|---|---|
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

## Validation

- `Compare exact duplicates, related-but-distinct items, and overlapping vocabulary with unrelated behavior.`

## Part

## Adapted objective

Compare normalized documented intent to find likely redundant behavior and missing abstraction opportunities.

## Constraints and boundary

Similarity narrows the search; evidence must retain element links and candidates remain distinct from confirmed redundancy.

## Validation and non-goal

Validation guidance: Compare exact duplicates, related-but-distinct items, and overlapping vocabulary with unrelated behavior.. This is a planned check, not a claim that the source command was run.

Non-goal: Do not automatically merge, delete, generalize, or mutate implementations..

## Fixture-authored outcome guidance

Expected: Reports expose evidence-backed candidates for a human-reviewed refactoring decision.

Alternative: Similar descriptions hide a meaningful behavioral difference.

Acceptable handling: record the condition, preserve source data, and stop affected follow-up.

## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Reports expose evidence-backed candidates for a human-reviewed refactoring decision.

**Alternative outcome 1:** Similar descriptions hide a meaningful behavioral difference.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Linked entities

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

