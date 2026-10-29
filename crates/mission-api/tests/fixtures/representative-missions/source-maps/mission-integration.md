# Mission integration — source-to-fixture map

Source index: [README.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/README.md)  
Source route: [ROADMAP.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/ROADMAP.md)

The nine Parts below follow the source roadmap's reviewed route. The source's
serial roadmap order is retained as display order. Native Mission validation
requires predecessor checkpoints; typed publication also materializes that
order contract. Those fixture-only edges do not assert that the original
independent work had these prerequisites. No child-Mission/cross-Mission
dependency is added.

| Source Part | Fixture waypoint | Preserved substance and adaptation |
|---|---|---|
| [05-mission-spec.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/05-mission-spec.md) | `mission-spec` | UUID domain contract, dedicated store, ownership/reference boundaries, migration and non-scheduling scope; represented as planned contract work, not a new live Spec. |
| [06-mission-tickets.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/06-mission-tickets.md) | `ticket-set` | Exactly four ticket-backed implementation surfaces, status ownership, exact read-backs, prerequisite reconciliation, and no scheduling Ticket. Linked Ticket/Spec IDs in the example are synthetic. |
| [13-structured-roadmap-contract.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/13-structured-roadmap-contract.md) | `roadmap-contract` | Native/typed schemas, stable IDs, order vs dependencies, revision labels, diagnostics, renderer and recovery; no unapproved schema change. |
| [07-mission-api.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/07-mission-api.md) | `mission-api` | Internal API/store, typed authoring, validation, stable IDs, revision-safe persistence and deterministic output; transport wiring and source mutation remain excluded. |
| [08-mission-transports.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/08-mission-transports.md) | `mission-transports` | Shared Mission facade operations, CLI/MCP parity, explicit roots, preview/mutation distinction, publication location and drift checks; no HTTP/viewer/scheduling. |
| [09-mission-session-attach.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/09-mission-session-attach.md) | `session-attachment` | Canonical Mission URNs and reference-only cross-session attachment while preserving session-local graphs; no scheduling or embedded state. |
| [10-dossier-migration.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/10-dossier-migration.md) | `dossier-migration` | Runtime enumeration, current-roadmap-only import, source/snapshot preservation, provenance, deterministic IDs and explicit blocked/skipped cases. No migration runs here. |
| [11-guidance-retarget.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/11-guidance-retarget.md) | `guidance-retarget` | Narrowly owned guidance updates and explicit requester authorization for takeover regardless of old session status; no inferred authority. |
| [12-validation-closure.md](../../../../../../../../transcripts/01-10-2026_roadmap-state-integration/12-validation-closure.md) | `validation-closure` | Evidence read-back, Ticket review, migration disposition, feedback and final lifecycle gates; synthetic green evidence is not acceptance. |

The typed-v3 authoring input retains all nine distinct Part documents and
planned outcome pairs. The generated Mission has three API-published states:
the first publication, the request/continuation state, and the changed
REQUEST state. Its `changes-requested` reaction is a separately stored,
fixture-authored event with fixed test metadata. It is not a genuine human
reaction, approval, signature, or authentication. The synthetic Ticket,
Spec, criterion, and Test evidence stores exist only under each temporary
workspace used by the generator and tests.
