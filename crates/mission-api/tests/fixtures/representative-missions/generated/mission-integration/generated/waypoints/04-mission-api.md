# W4. Implement durable Mission persistence (`mission-api`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 3; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [03-roadmap-contract.md](03-roadmap-contract.md) | Next: [05-mission-transports.md](05-mission-transports.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Ticket-backed: \`ce://default/ticket/00000000-0000-4000-8000-000000002002\` |
| **Child mission** | None |
| **State** | Owned externally |
| **Prerequisites** | roadmap-contract |
| **Session package** | mission-api |
| **Prompt** | Fixture-only adaptation: Implement UUID-backed validation, import, revision, archive, provenance, relationship resolution, and complete generated output in the internal API. Preserve the boundary: Use \`.workflow-tools/mission/\`; preserve typed Parts and original source records; keep transports out of the internal crate. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not expose transports or mutate source dossiers. |
| **Commit checkpoint** | Fixture review checkpoint: mission-api |

## Validation

- `Run focused persistence, validation, revision, exact renderer, history, and rejected-write-preservation tests.`

## Part

## Adapted objective

Implement UUID-backed validation, import, revision, archive, provenance, relationship resolution, and complete generated output in the internal API.

## Constraints and validation

Use `.workflow-tools/mission/`; preserve typed Parts and original source records; keep transports out of the internal crate. Validation guidance: Run focused persistence, validation, revision, exact renderer, history, and rejected-write-preservation tests.. Non-goal: Do not expose transports or mutate source dossiers..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Rejected writes preserve complete accepted bytes and every published revision remains self-contained and readable.

**Alternative outcome 1:** A revision conflicts with current accepted authority or an external reference is unavailable.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.


## Linked entities

Owning workspace: selected workspace root (local records in .workflow-tools/).

## Ticket: Synthetic representative Mission Ticket

Identity: ce://default/ticket/00000000-0000-4000-8000-000000002002

Ticket state (read-only): done

Execution mission-integration-synthetic-check (Test store: .workflow-tools/test): Passed; validation spec: representative-mission-synthetic-check; detail: Synthetic fixture evidence; no product validation is claimed.

## Specification: Synthetic representative Mission specification

Identity: ce://default/spec/00000000-0000-4000-8000-000000002003

Specification body (read-only): Synthetic local fixture record. Not a requester-approved specification.

### Criterion: mission-domain-boundary

Specification: [Spec 00000000-0000-4000-8000-000000002003](ce://default/spec/00000000-0000-4000-8000-000000002003); criterion: mission-domain-boundary

Synthetic criterion mission-domain-boundary for isolated fixture review.

