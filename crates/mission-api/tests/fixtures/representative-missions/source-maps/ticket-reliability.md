# Ticket reliability and migration — source-to-fixture map

Source index: [README.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/README.md)  
Source route: [ROADMAP.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/ROADMAP.md)

All seventeen selected Parts are mapped once in current source order. The
fixture keeps completed corrections separate from the still-blocked live
migration and the pending transport/documentation integration. It does not
replay the migration or treat historical reports as permission.

| Source Part | Fixture waypoint | Preserved substance and adaptation |
|---|---|---|
| [01-part-write-contract.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/01-part-write-contract.md) | `part-write` | Blank-write rejection, normalized deterministic filenames, lifecycle and amendment/legacy constraints; recovery/migration remain separate. |
| [02-recoverable-filesystem-publication.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/02-recoverable-filesystem-publication.md) | `recoverable-publication` | Ticket-local journal, staged writes, backups, durable ordering, idempotent recovery and complete old-or-new state; no store-wide migration selection. |
| [03-resolve-residual-test-failures.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/03-resolve-residual-test-failures.md) | `residual-test-scope` | Two distinct server-facing failures and explicit requester scope decision; no Mission mutation. |
| [04-fix-missing-prefix-diagnostics.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/04-fix-missing-prefix-diagnostics.md) | `missing-prefix` | Valid explicit store selection before missing-prefix diagnostics; production workspace behavior unchanged. |
| [05-fix-descendant-ticket-references.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/05-fix-descendant-ticket-references.md) | `descendant-refs` | Canonical child-store scan root, followable reference, and same-identity read-back; no broad discovery change. |
| [06-honor-default-workspace-reads.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/06-honor-default-workspace-reads.md) | `default-workspace` | Optional workspace on reads resolves to configured root while writes remain explicitly rooted. |
| [07-fix-board-mcp-workspace-fixtures.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/07-fix-board-mcp-workspace-fixtures.md) | `board-fixtures` | MCP board tests target their owning temporary store without relaxing production selectors. |
| [08-fix-move-mcp-workspace-fixture.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/08-fix-move-mcp-workspace-fixture.md) | `move-fixtures` | Source and destination remain explicit through move preflight/apply/read-back; no live migration. |
| [09-align-store-index-output-fixture.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/09-align-store-index-output-fixture.md) | `index-output` | Canonical input store and separately owned `.ticket/` generated catalog destination. |
| [10-fix-update-mcp-fixture-workspace.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/10-fix-update-mcp-fixture-workspace.md) | `update-fixture` | Valid initial lifecycle state, explicit temporary workspace, and same-ID read-back. |
| [03-resolve-ticket-test-gate.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/03-resolve-ticket-test-gate.md) | `test-gate` | Schema-valid stored states, focused fixture corrections, full package gate and explicit boundary for unrelated failures. |
| [03-part-layout-migration.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/03-part-layout-migration.md) | `part-layout` | Dry-run/apply/verify/undo, fingerprints, exact cleanup eligibility, recovery and no live store operation. |
| [11-fix-migration-cli-dispatch.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/11-fix-migration-cli-dispatch.md) | `cli-dispatch` | Runtime dispatch, explicit root, readable dry-run report and unchanged ticket bytes. |
| [12-scope-part-layout-migration.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/12-scope-part-layout-migration.md) | `migration-scope` | Selected-store-only plans and pre-mutation rejection of out-of-store paths; normal descendant listing remains unchanged. |
| [14-root-store-migration-preflight.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/14-root-store-migration-preflight.md) | `root-preflight` | Fresh root/nested reports and preserved uncertainty about original root error; no apply, index rebuild, or ownership claim. |
| [04-run-approved-store-migration.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/04-run-approved-store-migration.md) | `approved-apply` | W16 live apply remains blocked pending repair and separate exact-operation approval; no authorization inferred from reports. |
| [05-transport-documentation-integration.md](../../../../../../../../transcripts/12-10-2026_ticket-part-empty-files/05-transport-documentation-integration.md) | `transport-docs` | Cross-transport shared validation/recovery, focused compatibility, direct docs and read-back gates remain pending. |

The synthetic Mission state marks the first fifteen mapped waypoints complete,
`approved-apply` blocked, and `transport-docs` pending. This preserves the
source's blocker and approval boundary as review data only; the Ticket and
validation records are isolated synthetic entities. The native-v2 input gives
each waypoint a distinct objective, constraint, validation, non-goal, expected
outcome, and alternative with safe handling. Mission-required predecessor
checkpoints are a renderer adaptation, not claimed source dependencies.
