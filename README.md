# Mission

## Review a generated Mission

**Novice path:** Start from the Mission's `generated/README.md` and choose the
[Mission Review Agent](../.agents/agents/mission-review.agent.md) in the agent
picker. Provide that entry path; the role follows its linked Mission artifacts
and reports evidence-backed gaps without running product tests or claiming
readiness from publication alone. If the entry is missing or incomplete, the
review is blocked rather than reconstructed from hidden state.

**Power-user path:** Invoke the Mission Review Agent with the exact
`generated/README.md` path or URI and, when needed, the relevant acceptance
criteria link. It checks the linked strict accepted Mission, inventory,
references, and existing validation receipts without executing tests. For the
complete, missing, contradictory, and incomplete-entry examples, see the
[manual walkthrough and synthetic fixtures](../test/docs/mission-reviewer-role/manual-walkthrough.md).
These examples validate reviewer behavior only; they are not product
acceptance evidence.

## Direct authoring and operator evidence

The `mission` package provides the `mission` binary with feature `cli` and the
`mission-mcp` stdio server with feature `mcp`. Both transports use the same
[facade operations](crates/mission/src/lib.rs). No dossier, migration, or bundle
file is required for direct authoring.

### Bounded schema and read-only entity composition

`mission-api::SharedEntity` is a validated, read-only intersection, **not** a
replacement entity store or a new Ticket format:

| Field | Meaning and allowed values |
|---|---|
| `schema_version` | Shared projection version, integer `1` only. |
| `source_schema` | `mission-bundle-v1`, `mission-bundle-v2`, `mission-source-v2`, `mission-source-v3`, or `ticket-unversioned`; never inferred/coerced from unknown versions. |
| `id` | Non-nil UUID: `AcceptedMission.mission_id`, `TypedDraft.mission_id`, or the existing `TicketManifest.id`. |
| `title` | Nonblank string: Mission title or Ticket's typed string `extra.title`. No normalization or domain write occurs. |

These are the only demonstrated common identity/typed-attribute semantics.
Native bundle schemas `1`/`2`, typed Mission sources `2`/`3`, and Part source `1` keep
their separate meanings. Ticket's existing `EntityManifest` has no native
schema-version/revision field; `ticket-unversioned` does not invent one.
Mission revisions, objective, progress, Ticket states/types/plan revisions,
timestamps, and arbitrary extras remain domain-owned. Incomplete drafts can
remain incomplete in their domain; a blank title cannot be projected as a
validated shared entity.

Use `SharedEntity::from_mission`, `from_typed_mission`, `from_ticket`, or strict
`from_json`. Unknown keys, wrong types, malformed identities and unsupported
versions fail. `SharedSchemaError` retains the exact input bytes, including
unknown data, alongside a diagnostic. No projection write-back or implicit
migration exists. For native Mission v1 → typed source v2, use the existing
`MissionLifecycle::{plan_migration,apply_migration,read_migration,resume_migration,rollback_migration}`
journal workflow. Planning rejects unsupported native manifests and existing
unreadable/unsupported typed sources; applying refuses a source that appeared
after planning. Failures never replace that source with a guessed v2 record.
Successful conversion preserves UUIDs, Requirement links and immutable native
history, and advances the existing shared CAS revision. No Ticket/Spec migration
or historical repair is performed.

### Lifecycle authority and recovery

`mission::WorkspaceLifecycle` provides checked activation, pause/resume,
blocking, cancellation, waypoint/Mission completion, result review, Session
handoff and latest-current-completion reopening. Shared state/control contracts
remain in `memory-kernel::lifecycle`; native Ticket state remains Ticket-owned.
Mission pause/cancel gates execution without cascading Ticket edits.

Controls use the Mission publication lock, control-sequence CAS and append-only
records. A pending cross-store reopen/handoff gates entry and route replacement;
retry the same operation and preconditions to recover its persisted subset.
Reopen selects the greatest current completion sequence, preserves evidence,
uses Ticket's audited expected-done/revision reopen API, invalidates approval
and marks dependent completion evidence stale. Resume requires valid approval
and prerequisite evidence.

Raw `record_progress` cannot mark a waypoint completed. Use validated lifecycle
completion with current evidence; historical completed states remain readable
and repairable. `ApprovalTransaction` record writers are trusted cross-crate
facade persistence primitives, not agent-facing operations.

### Session intake and managed planning Mission

`mission::intake::submit_and_create` stores submitted bytes append-only in
`sessions/<id>/intake/<request_id>/input.bin`, re-verifies their SHA-256 on read,
and creates one planning draft under the explicit workspace root's existing
`.workflow-tools/mission/missions/<uuid>/` layout. The Mission keeps an immutable
copy at `input/submitted-input.bin` plus a Session reference in `input/intake.json`.
The capture transcript is unchanged. The Session mutation lock is held across
submission, reservation and linkage, excluding concurrent finish (`busy`).
New or unlinked intake after finish returns `session_finished`; linked intake
remains readable by replay.

The same roots, Session, request ID and bytes resolve the same Mission UUID and
paths across restarts. Different bytes return `idempotency_conflict`.
The Session journals `Reserved` before Mission creation and `Linked` only
after provenance exists. Interrupted operations expose `pending` with the
persisted subset. The verification hook simulates stops between steps, not
arbitrary crashes inside a step.

Draft metadata is explicitly intake-derived, pending later planning. The
receipt exposes the generated entry as `NotYetGenerated` until publication;
symlinked or unsupported generated paths fail explicitly. Intake creates no
approval or lifecycle journal and grants no execution authority. Structured
failures distinguish `session_not_found`, `intake_missing`, `session_finished`,
`workspace_unavailable` and `incompatible_store`. Later planning edits preserve
the immutable provenance used by `find`.

Typed Mission writes take an OS file lock (`.<id>.typed.flock`, never unlinked,
released by the kernel on crash; waits end in `typed-lock-timeout`).
Unknown legacy `.<id>.typed.lock` directories are preserved and reported as
`PublicationRecovery` after a short grace. Mutual exclusion covers this build's
callers only: quiesce older writers before upgrading.
A new Mission persists a synced `.<id>.typed.intent` before staging. Partial
stages are rebuilt only with matching intent and verified request content;
an exact complete stage can be adopted. Foreign, corrupt, symlinked or
differing stages and intents block creation without being deleted.

### Planned waypoint outcomes

Native bundle **v2** and typed source **v3** require `outcome_plan` on every
waypoint. Native v1 and typed v2 retain their absent plan and unchanged output;
legacy authoring entry points still create those legacy formats. To upgrade,
explicitly submit the new version and complete requester-authored plans for
every waypoint through native import/publication or typed save/publication
(the same inline `draft` object in CLI and MCP). No read, repair, or migration
invents a plan or upgrades a legacy version.

```json
{
  "outcome_plan": {
    "expected_outcome": "The accepted source and generated roadmap agree.",
    "alternative_outcomes": [{
      "outcome": "Validation detects a mismatch.",
      "acceptable_handling": [
        "Preserve the accepted source and inspect the diagnostic.",
        "Request a replan."
      ]
    }]
  }
}
```

Expected and alternative outcomes must be nonblank. There must be at least one
alternative and each must have at least one nonblank handling option. Legacy
versions reject supplied plans; new versions reject absent, malformed, or
incomplete plans. Unsupported versions also fail before publication, preserving
accepted/source/generated bytes.

The journaled native-to-typed migration maps v1 → v2 without a plan and
v2 → v3 with exact plan values. Typed storage, native edits, repair candidates,
and revision archives retain those versions and plans. Renderer version
depends on the projection:

| Projection | Renderer |
|---|---|
| Unlinked native v1, legacy output | `mission-api/5` |
| Unlinked native v2 with planned outcomes | `mission-api/6` |
| Any relationship-bearing projection (Ticket, Spec/criterion, or Test/evidence references) | `mission-api/7` |

Version 7 replaces the serialized canonical absolute workspace path with the
stable, truthful context label `Owning workspace: selected workspace root
(local records in .workflow-tools/).` It does not identify a workspace
globally or select a different root. Every linked record is still resolved
only beneath the caller's explicit workspace root using the existing
canonical-path checks; wrong-root, missing-reference, and symlink escapes
remain errors, with no index, ancestor, or other-workspace fallback.

Existing published v5/v6 projections and their archived contracts are
historical bytes and are not rewritten. If current linked relationships now
render as v7, `check-generated` may report their older projection as drift;
update it only by explicitly republishing through the owning Mission
operation. The Part source, provenance, and output-contract format versions
do not change.

Roadmap and waypoint Parts render ordered, escaped **manual guidance**.
Alternatives are not acceptance criteria, branches, progress or state changes,
dependency edges, scheduling, dispatch, or automatic recovery. Actual
`operator_results` remain separate, with unchanged validation and rendering.

Native bundle Requirements can add reference-only `canonical_criteria`:

```json
{
  "id": "mission-schema",
  "text": "Use the specification's schema contract",
  "deferred": false,
  "canonical_criteria": [{
    "specification_id": "b2c52f3b-5061-441e-9c7b-dc8dc8d8eec8",
    "criterion_id": "mission-shared-schema-version-validation"
  }]
}
```

This is the reference shape, not an assertion that the illustrative criterion
is present in that Spec. Requirement IDs keep their existing stable lowercase
slug rules and are independent of display order. `CriterionReference` carries a
typed specification UUID and `CriterionId`, an opaque nonblank Spec-owned ID
(not restricted to Mission slug syntax). Empty reference lists remain valid
for legacy requirements; no links or criteria are fabricated. Duplicate links,
nil specification UUIDs and blank criterion IDs fail validation.

`reference.resolve(explicit_workspace_root)` reads only
`.workflow-tools/spec/specs/<specification_id>/spec.toml` in that workspace.
It checks the manifest UUID and exactly one matching canonical criterion:
`acceptance_criteria[].id` for unversioned Specs, or `criteria[].criterion_id`
for Spec format `2`, using Spec's existing typed models. Malformed arrays,
unsupported explicit Spec formats, dangling IDs, IDs belonging only to another
Spec/root, and symlink redirection fail. No index/ancestor/other-workspace
fallback or store initialization occurs. All Mission authority writes enforce
resolution before replacement; failed updates preserve source, history and
generated output. References use the owning workspace's existing local
`ce://default/spec/<uuid>` convention; cross-workspace resolution is not added.
Criterion IDs remain separate typed data, not a new URI/criteria authority.

Pure embeddable renderers accept the existing typed entities:
`render_mission_entity`, `render_roadmap_entity`, `render_ticket_entity`,
`render_specification_entity`, `render_criterion_entity`, and
`render_requirement_entity`. Resolve a criterion first, then render it with
its reference; mismatched Spec/criterion identities fail. Returned strings can
be concatenated in caller-selected order. Requirement blocks/rows carry
canonical Spec links plus criterion IDs, never copied authoritative criterion
statements. The native roadmap reuses `render_requirement_row` and
`render_criterion_reference`. Text fields escape HTML and Markdown punctuation
deterministically; authored Part Markdown retains its existing document
semantics. Rendering does not save projections or mutate Mission, Ticket,
Spec, criterion, test, or lifecycle state.
Renderer versions retain entity-driven waypoint relationships and
escaping/canonical-reference output while the whole-state layout below is
shared across versions. Older generated bytes can report drift; they are never
rewritten by these read-only APIs.

Acceptance gate (from the meta-workspace):

```sh
cargo test --manifest-path workflow-tools/Cargo.toml --locked -p mission-api --test schema_requirement_renderer_contract
```

### Optional waypoint interoperability

A waypoint remains valid without a Ticket. Promotion uses the existing
`ownership_refs.ticket_id` / `ticket_uri` in typed authoring (or native
`ticket_ref` / ticket-backed scope), referencing an **existing** Ticket.
When both typed Ticket fields are supplied they must identify the same Ticket.
Promotion never creates, activates, changes ownership of, or adds dependency
edges to a Ticket, including one already active or completed. Ticket-backed
waypoint state remains externally owned.
Native-to-typed migration parses canonical Ticket and child-Mission URIs
against the selected workspace, retains each URI verbatim alongside its UUID,
and preserves relationship references. Scope-only Ticket references remain
Ticket-backed. Malformed, nil, conflicting or wrong-workspace ownership links
fail explicitly rather than silently dropping their typed identities.

Both `TypedWaypoint` and native `WaypointFragment` accept the same optional
reference-only field, omitted when empty:

```json
{
  "relationships": {
    "specification_ids": ["88a07686-34da-4ec9-a569-21eb286e7609"],
    "criteria": [{
      "specification_id": "88a07686-34da-4ec9-a569-21eb286e7609",
      "criterion_id": "mission-waypoint-optional-promotion"
    }],
    "executions": [{
      "execution_id": "example-green",
      "criterion": {
        "specification_id": "88a07686-34da-4ec9-a569-21eb286e7609",
        "criterion_id": "mission-waypoint-optional-promotion"
      }
    }]
  }
}
```

These illustrative IDs must actually exist in the selected workspace; the
example is not execution evidence. All lists default to empty and require
unique identities. `executions[].criterion` is optional. When supplied, both
its Spec UUID and criterion ID must also be linked by the Test-owned execution.
Criterion resolution reuses `CriterionReference` and its strict Spec-owned
criterion lookup. No criterion statement or execution payload is saved in
Mission source, and no second criteria/execution authority is introduced.
The relationship itself associates an execution with the stable waypoint;
it does not mutate the Test record or imply a passed outcome.

All links resolve within the explicitly selected owning workspace, using
canonical `.workflow-tools/{ticket,spec,test}` paths only. Ticket manifests are
read through `TicketFs`, Specifications through `SpecStore`, and execution
outcomes through `TestStoreConfig`. A Ticket's `related_specs` are also resolved:
supported representations are legacy comma-separated UUIDs, UUID string arrays,
or existing `SpecRef` objects with `spec_id`, local `workspace` (`default` or the
selected workspace name), and `store_root: ".workflow-tools/spec"`.
Spec API lookup must point to the exact canonical Spec directory; no ancestor,
other-root, alias-index, legacy-layout or symlink fallback is attempted.

Saves/imports/publications resolve before replacing authority. Missing entities,
wrong roots, nil/mismatched UUIDs, dangling criteria, duplicate references,
malformed records and mismatched execution-criterion links return explicit
`mission-waypoint-link-*` or existing `mission-requirement-*` diagnostics.
Failures preserve accepted/source records, authored Parts, immutable history
and generated output. Validation and rendering also recheck current links,
so deleting an external entity cannot silently generate a partial roadmap.

Store-backed publication, typed `render_mission`, and
`render_in_workspace(workspace, bundle, mission_id)` read current linked
Ticket titles/states, Spec titles/bodies, criteria and execution outcomes/details.
Roadmap/Part projections include canonical IDs and owning-workspace context.
They are read-only snapshots, not authority copies. Rootless pure renderers
retain their static legacy Ticket-reference behavior; new `relationships`
require the explicit-workspace renderer and fail with
`mission-waypoint-link-root-required` otherwise.
No cross-workspace routing, generic workflow backend, historical repair, live
Mission mutation or automatic Ticket/Test execution is provided.

Acceptance gate (from the meta-workspace):

```sh
cargo test --manifest-path workflow-tools/Cargo.toml --locked -p mission-api --test waypoint_interoperability_contract
```

### Selecting the workspace

Both transports select their owning workspace in this order:

1. `--workspace <path>` on the executable.
2. The optional `MISSION_WORKSPACE` environment variable.
3. The process working directory, preserving the historical default.

Relative values resolve against the process working directory. An explicitly
selected empty value or a path containing a symlink is rejected. A valid
explicit `--workspace` takes precedence even if `MISSION_WORKSPACE` is empty;
there is no fallback after a selected value is rejected. Domain API calls
always receive an explicit resolved workspace and never read the environment.
Read operations do not initialize a missing store.

For a portable canonical launch from the meta-workspace root:

```sh
MISSION_WORKSPACE="$PWD/workflow-tools/mission" mission-mcp
MISSION_WORKSPACE="$PWD/workflow-tools/mission" mission get MISSION_UUID
```

An explicit `--workspace` overrides this configuration. `mission-mcp` accepts
the same `--workspace <path>` launch option.

Create a nonempty Mission with inline waypoint content:

```sh
mission --workspace /absolute/workspace create-mission \
  "Installation check" "Verify an isolated installation" \
  '[{"waypoint_id":"verify","title":"Verify binaries","prompt":"Run the installed binaries and retain their output.","validation":[{"command":"mission --help","cwd":"/isolated/install"}],"commit_checkpoint":{"message":"none (runtime evidence only)"},"non_goals":["Do not replace the stable installation"]}]'
```

Waypoints require `waypoint_id`, `title`, `prompt`, nonempty `validation`
commands, and `commit_checkpoint.message`. Optional `depends_on` and
`non_goals` default to empty lists. Each waypoint after the first must depend
on its immediate predecessor. IDs remain stable; W1/W2 labels follow the
accepted execution order. Unknown fields and incomplete input are rejected.

Use the returned Mission UUID and current revision for subsequent writes:

```sh
mission --workspace /absolute/workspace get MISSION_UUID
mission --workspace /absolute/workspace edit-mission-objective MISSION_UUID \
  "Updated installation objective" EXPECTED_REVISION
mission --workspace /absolute/workspace add-waypoint MISSION_UUID record \
  "Record evidence" EXPECTED_REVISION --depends-on verify \
  --prompt "Retain actual command output and record its location." \
  --validation-command "mission --help" --validation-cwd /isolated/install \
  --checkpoint "none (runtime evidence only)"
```

`edit-waypoint` uses the same positional arguments and content flags as
`add-waypoint`; content replacement preserves local progress and stable IDs.
Repeat `--validation-command` or `--non-goal` for multiple values.
Ticket-backed and child-Mission waypoints retain external authority: local
content/progress edits are rejected.

Reordering can supply dependency changes in the same revision, avoiding an
invalid intermediate predecessor chain:

```sh
mission --workspace /absolute/workspace order-waypoints MISSION_UUID \
  record,verify EXPECTED_REVISION \
  --waypoint-dependencies '{"record":[],"verify":["record"]}'
```

The optional map replaces only the named waypoint dependencies. Dependencies
are never silently removed; unknown IDs, invalid orders, cycles, and invalid
content reject the entire update without changing accepted data.

```sh
mission --workspace /absolute/workspace record-progress MISSION_UUID verify \
  completed EXPECTED_REVISION
mission --workspace /absolute/workspace record-validation-result MISSION_UUID \
  verify passed "mission --help" "/persistent/evidence/mission-help.log" \
  EXPECTED_REVISION
mission --workspace /absolute/workspace validate-mission MISSION_UUID
mission --workspace /absolute/workspace publish-mission MISSION_UUID EXPECTED_REVISION
mission --workspace /absolute/workspace check-generated MISSION_UUID
```

Every successful write advances the revision. Stale writes leave the accepted
record unchanged. Result outcomes are `passed`, `failed`, or `blocked`, with a
nonblank operator command and evidence pointer. Recording a result neither
executes the command nor changes waypoint progress. Operators run commands
separately and are responsible for genuine evidence.

`publish-mission` publishes the stored content and generates the native
projection, including operator results. `get` (alias `get-mission`) returns
accepted content, local/derived progress, evidence, and checked
`publication_state`: `not-published`, `current`, or `drift`. A content write
requires publication again; generated Markdown is never an authoring surface.

### Generated review entry and verification

Start at `generated/README.md` in the selected Mission directory. It links the
roadmap, every generated Part and declared evidence, and distinguishes promises,
delivery and verification from unknown product facts. Workspace file artifacts
are resolved from the workspace root and linked relative to this entry;
canonical `ce://` references remain canonical. Publication rejects
outside-workspace references and symlink escapes; a read-only render without a
workspace reports such links as unavailable, never as absolute file links.
Read-only rendering of a historical reference that is now missing, unsupported
or outside the workspace also marks it explicitly unavailable in the entry,
roadmap and Parts. This does not authorize accepting that reference on a write.

Each persisted revision also records
`history/provenance-revision-<revision>.json`. This immutable record identifies
the `workflow-tools/mission-api` producer, purpose, invocation, grouping,
inputs and outputs, and validates every declared file's authored/generated/
helper role, format and schema guarantee. Reads do not synthesize provenance
for older revisions. Publication rejects missing, unsupported, wrong-workspace
or escaping file references before replacing source or accepted data; canonical
`ce://` references remain owned by their respective domains and are not
resolved by Mission. `resolve_artifact_reference(workspace_root,
consumer_path, reference)` produces a consumer-relative link so each supported
Markdown destination resolves from its actual location.

Publication, repair fingerprints and verification share one generated inventory
containing the README, roadmap, history, waypoint map and Parts. An absent external-status snapshot and
the empty snapshot have the same explicit empty representation, so typed
publication and subsequent verification agree byte-for-byte. Transport success
does not imply acceptance of historical review rows.
The projection renderer is `mission-api/5` for unlinked native v1,
`mission-api/6` for unlinked outcome-bearing native v2, and `mission-api/7`
when relationships are present. Older generated output reports drift until
explicitly republished through the owning Mission operation.

`check-generated` reports `generated-changed`, `generated-missing`,
`generated-extra`, `generated-dangling` and `generated-destination-invalid`
separately. The latter includes redirected generated files or destinations
outside the owning workspace. Checks are read-only; regenerate through the
owning publication operation rather than editing projections.

### Representative generated fixture corpus

The [representative Mission fixture review index](crates/mission-api/tests/fixtures/representative-missions/README.md)
links API-generated 4-, 9-, and 17-waypoint examples, their source maps,
adaptations, history, and review focus. The same authoring inputs and persisted
outputs are exercised by the integration test. From the meta-workspace root:

```sh
# Read-only tests; do not rewrite committed examples.
cargo test --manifest-path workflow-tools/Cargo.toml --locked -p mission-api --test representative_mission_fixtures

# Explicitly regenerate the persisted review outputs through Mission APIs.
cargo test --manifest-path workflow-tools/Cargo.toml --locked -p mission-api --test representative_mission_fixtures regenerate_reviewable_outputs -- --ignored --exact
```

The normal test compares complete generated file trees and bytes across fresh
isolated roots, without normalization. The ignored regeneration command is an
explicit authoring action; inspect its diff before retaining regenerated
output. Fixture references and reactions are synthetic and do not represent
live records or genuine human approval.

### Whole-state output history and human reactions

Every new authority revision captures its complete canonical file state under
`history/states/revision-N/`, including accepted data, any typed source/Parts,
request material, provenance and any generated documents. The same shared
`revision-N` label identifies every file in that state; occupied labels
(including empty directories) fail without replacing authority or history.
Existing `history/revision-N.json`, typed-source history and provenance remain
immutable. Older missing whole-state archives are **not** rebuilt from current
source or today's renderer.

Published files retain `generated/README.md` and `generated/ROADMAP.md`.
Current Markdown Parts are `generated/waypoints/NN-<id>.md`, with a nearby
`generated/waypoints/README.md` map. `generated/HISTORY.md` is generated from
validated preserved states and explicitly lists unrecoverable gaps, using
ranges for large gaps. Pure previews instead report unknown archive availability.
Archive BOM paths are relative to that archived state: its canonical bytes can
be reconstructed without re-rendering. Archived Markdown retains original
consumer-relative links; it is not rebased for browsing inside the archive.
History and separately stored reactions are intentionally outside each state's
recursive inventory.

`MissionStore::publish_with_inputs` and CLI `publish-with-inputs` atomically add
request material with publication. The CLI takes the existing Mission UUID,
bundle manifest path, expected current revision, then a strict JSON object:

```json
{
  "merged_request": "The merged request text",
  "continuations": {
    "initial.md": "Initial input",
    "follow-up.md": "Independent continuation"
  }
}
```

Only the merged request is written as root `REQUEST.md`. Named continuations
are create-new files under root `inputs/`; they persist independently across
revisions, never overwrite one another or become per-input revisions.
Dossier migration captures an existing `REQUEST.md` and `inputs/` tree,
rejects symlinks, and leaves original dossier bytes untouched. It does not infer
missing inputs or import historical approvals. Typed migration rollback now
appends a native compensation revision rather than deleting the migrated state;
its original snapshot, generated-state archives and reactions remain available.

Before any reaction, root `output-contract.json` records schema version, Mission
identity, revision/label, renderer, producer, purpose and a complete sorted BOM
of canonical files with SHA-256, byte size, role and format. The BOM excludes
the contract itself and recursive history/events. Root
`artifact-provenance.json` is captured in the BOM and archives alongside the
existing immutable provenance record. Contract metadata is reproducible and
does not invent a historical capture timestamp.

`mission --workspace ROOT get-output-contract UUID` validates current bytes and
returns `output_record.record.contract` plus `output_record.record.binding`.
Use that exact `sha256:<digest>` binding in a **separate** reaction JSON:

```json
{
  "revision": 4,
  "decision": "changes-requested",
  "signature": "sha256:DIGEST_OF_EXACT_CONTRACT_JSON_BYTES",
  "actor": "requester-provided identity",
  "actor_source": "explicit review response",
  "metadata": {"channel": "review", "note": "Clarify the validation evidence"}
}
```

Submit it with `mission --workspace ROOT record-approval-event UUID 'JSON'`.
Only `approve` and `changes-requested` are supported. The API assigns a new
event UUID and the actual recording time (`recorded_at_unix_ms`); callers cannot
supply/backdate either. One create-new JSON event per revision lives at
`history/events/revision-N.json`. `get-approval-event UUID N` reads it back.
The event contains decision, binding and event metadata, **not** a copied BOM.
`signature` is only a non-authenticating contract identity/digest: neither
actor identity nor cryptographic signing is verified. Recording a reaction
changes no Mission/Ticket/Spec authority, progress, execution permission or
readiness claim.

Invalid JSON/unknown fields, unsupported decisions, missing contracts, complete
BOM/digest/size mismatches, stale revision, replay and occupied archives fail
without overwriting previous state. A later revision needs its own fresh
reaction; old decisions are never carried forward. `check-generated` also
validates current/archive contracts and stored reactions; failures surface as
`mission-output-history-invalid`. Repair preview fingerprints include these
files and predict the full new snapshot/contract; repair never silently fixes
old history or decisions.

MCP uses the same facade operations: `publish-with-inputs` with `output_inputs`,
`get-output-contract`, `record-approval-event` with `approval_event_json`, and
`get-approval-event` with `revision`. They all require `mission_id`; publication
also requires `manifest_path` and `expected_current_revision`.

Acceptance gate (from the meta-workspace):

```sh
cargo test --manifest-path workflow-tools/Cargo.toml --locked -p mission-api --test revision_history_approval_event_contract
```

### Journaled canonical repair

Repair is an explicit recovery operation for an accepted Mission whose typed
source or derived data has diverged. It never runs implicitly as part of
reading or authoring. The CLI uses the selected `--workspace` as the repair
root. Its required `--records` argument is the complete strict `RepairRequest`
JSON envelope with `source_root`, `records`, and `policy`; it is not a bare
array. The envelope's `source_root` must resolve to the same root as
`--workspace`.

Each record contains `mission_id`, `expected_revision`, `accepted_sha256`,
`source_sha256`, and `payload_fingerprint`. A first, read-only preview with
empty hash fields reports the observed values and blockers. Re-run preview
with those exact observed values, inspect every candidate and blocker, and
apply only the returned proposal identity:

```sh
mission --workspace /absolute/workspace repair-preview \
  --records '{"source_root":"/absolute/workspace","records":[{"mission_id":"MISSION_UUID","expected_revision":4,"accepted_sha256":"OBSERVED_ACCEPTED_SHA256","source_sha256":"OBSERVED_SOURCE_SHA256","payload_fingerprint":"OBSERVED_PAYLOAD_FINGERPRINT"}],"policy":"forward-authoritative-repair"}'
mission --workspace /absolute/workspace repair-apply \
  --records '{"source_root":"/absolute/workspace","records":[{"mission_id":"MISSION_UUID","expected_revision":4,"accepted_sha256":"OBSERVED_ACCEPTED_SHA256","source_sha256":"OBSERVED_SOURCE_SHA256","payload_fingerprint":"OBSERVED_PAYLOAD_FINGERPRINT"}],"policy":"forward-authoritative-repair"}' \
  --proposal-id sha256:PREVIEW_ID
mission --workspace /absolute/workspace repair-journal --journal-id JOURNAL_UUID
mission --workspace /absolute/workspace repair-resume --journal-id JOURNAL_UUID
mission --workspace /absolute/workspace repair-rollback --journal-id JOURNAL_UUID
```

Preview fingerprints every regular file under each selected Mission, including
history, Parts, projections, and provenance. Apply rechecks that fingerprint
under the Mission write lock, persists its journal and recovery snapshot
before writing a forward revision, and stops on drift. A fully conformant
selection is a no-op and creates neither journal nor revision. Resume and
rollback require the actual journal UUID and the same workspace; rollback
appends a compensation revision rather than restoring the old tree.

### Shared lifecycle projections and capability contracts

`memory_kernel::lifecycle` defines the six shared states (`planned`, `active`,
`paused`, `blocked`, `completed`, `cancelled`), schema parsing, basic control
defaults and independently implementable capability traits.
`mission_api::lifecycle` re-exports these contracts and owns only Mission
identities, journal handling, projections and `LifecycleSnapshot` (state plus native
domain/state/revision, owning root, control sequence and separate execution
eligibility). The kernel contracts separate read, transition, planning,
approval, pause/resume, cancellation, completion/result-review, handoff and reopen.
Implementers bind their domain identity, snapshot and error through
`LifecycleSubject`; the generic traits no longer depend on Mission types.
Native conversion helpers are provided by the domain `LifecycleStateExt`
extension trait, which callers must import to use its associated functions.
Ticket vocabulary is mapped by `ticket_api::TicketLifecycle`; the Mission
extension delegates to that adapter rather than maintaining a second mapping.
Unsupported capabilities return `unsupported_capability`; nothing pretends to
succeed. The `mission` facade's `WorkspaceLifecycle` reads Ticket state from
the explicit workspace Ticket store and transitions it only through the Ticket
domain (`foreign_transition_rejected` when Ticket refuses); Mission never writes
Ticket state.

- Ticket `planning`/`ready` → planned, `in-implementation`/`in-review` → active,
  `on-hold` → paused, `blocked` → blocked, `done` → completed, `cancelled` →
  cancelled; any other state fails with `ticket_state_unmapped`.
- Single-session `Pending`/`InProgress`/`Completed`/`Blocked` → planned/active/
  completed/blocked. Paused/cancelled come only from Mission-owned control events.
- A Mission is planned until work starts, active while work runs or results await
  review (a legacy all-completed Mission without an explicit completion event is
  active with `result_review_needed`), blocked at an unavailable required entry,
  and completed only by an explicit completion event. Precedence: terminal
  cancel/complete, pause, blocked entry, planned/active.
- Mission pause/cancel deny the separate `eligibility` field only; a Ticket-backed
  waypoint keeps its Ticket state.

The version-1 control journal is `lifecycle-journal.json` in the UUID Mission
directory: `{"version": 1, "events": [...]}` with `seq`, `op_id`, `target`
(`mission` or a waypoint), `prior_seq`, `event` (`pause`/`resume`/`cancel`/
`complete`) and `prior_state`. W1 defines schema, validation and projection
only; durable writes and recovery belong to a later slice. A missing journal means
no controls (never approval); corrupt content, unsupported versions and
conflicting histories are explicit errors. Control events do not change the plan
manifest or semantic revision.

### MCP contract

Start `mission-mcp` with its owning workspace as cwd, or choose it with
`--workspace`/`MISSION_WORKSPACE`. Initialize the real stdio
JSON-RPC connection, send `notifications/initialized`, then call `tools/list`
and `tools/call`. The executable's `--help` documents launch options; it does
not replace the real MCP protocol checks.

The `mission_operation` tool supports `create_mission`,
`edit_mission_objective`, `add_waypoint`, `edit_waypoint`, `order_waypoints`,
`record_progress`, `record_validation_result`, `validate_mission`,
`publish_mission`, and `get` (alias `get_mission`), plus the existing bundle
operations. Existing-record writes require `mission_id` and
`expected_current_revision`.

`create_mission` accepts `title`, `objective`, and `waypoints` with the same
inline content as the CLI. Add/edit use `waypoint_id`, `waypoint_title`,
`depends_on`, `prompt`, `validation`, `commit_checkpoint`, and `non_goals`.
Reorder uses `execution_order` and optional `waypoint_dependencies`.
Progress uses `waypoint_id` and `state` (`pending`, `in-progress`, `completed`,
or `blocked`); evidence uses `waypoint_id`, `outcome`, `operator_command`,
and `evidence_pointer`. Inspect both JSON-RPC errors and returned Mission
`status`/`diagnostics`; a completed transport call is not domain success.
Repair operations are `repair_preview` and `repair_apply`, with a structured
`repair_request` containing `source_root`, `policy`, and the same `records`
array used by the CLI. Apply also requires the exact `proposal_id` returned by
preview. `repair_journal`, `repair_resume`, and `repair_rollback` take the
actual `journal_id`.

### Granular planning operations and review plan

`MissionLifecycle::apply_plan_operation` applies `SetGoal`, `CreateWaypoint`,
`UpdateWaypoint`, `RemoveWaypoint`, `ReorderWaypoints`, `SetRequirements` or
`RecordEvidence` to a published typed Mission using `expected_revision` and
`request_id`.

- Stale revisions return `revision_conflict`; unknown waypoint IDs return
  `waypoint_not_found`. Rejected requests preserve authoritative and projection
  bytes.
- Identical request retries return the recorded outcome with `replayed: true`
  without writes. Conflicting reuse returns `request_id_conflict`.
- Removing referenced work requires a replacement with a new stable ID in the
  same operation (`replacement_required`). Requirement ownership transfers to
  the replacement. Orphaning required ownership fails (`unowned-requirement`).
  Completed or result-bearing identities are retained
  (`retained-waypoint-required`).
- Schema 3 requires complete outcome plans; schema 2 remains legacy without
  invented outcomes or requirements.
- Source, accepted state, history, `plan-requests/`, `evidence-bindings.json`
  and `review-plan.json` publish in one atomic swap, preserving existing intake
  provenance and lifecycle journals. Repair includes the review projection in
  its swap and fingerprints.
- Each waypoint depends on its immediate predecessor in the total order.

`MissionLifecycle::review_plan` never writes and reads a coherent revision
under a shared lock on the Mission's existing `.<id>.typed.flock`.
It exposes the goal, ordered waypoints, prompt, Part,
checkpoint, requirements, non-goals, references, declared validation gates,
`plan_digest` and `evidence_identity`. The semantic digest excludes evidence.
Evidence stays bound to its original digest after an edit, with `current:
false`. `review_status: "evidenced"` means operator evidence, not requester
review: `formal_review` remains `not-recorded`. Approval is a separate contract.

Reads never create the lock file and never follow it through a symlink. An
absent Mission reports `NotFound`. A present Mission whose lock file is
missing, a symlink or not a regular file, or that still has a legacy
`.<id>.typed.lock`, returns a `PublicationRecovery` error instead of an
unguarded read. Run a typed write or repair, or inspect the legacy lock once no
older writer is running, to restore it. A writer holding the lock makes the
read wait up to 30 seconds (`typed-lock-timeout`).

### Revision-bound requester approval

Mission-linked Ticket execution entry requires a grant bound to the reviewed
plan revision, `plan_digest` and review snapshot. `mission::approval` is a
trusted library facade, not an agent-callable receipt-minting tool.

1. Record a passing cold walkthrough, followed by a formal review from a
   different reviewer that names its `cold_review_id`. Only the latest
   cold/formal chain for the current snapshot supports a challenge.
2. `prepare_challenge` journals the binding under
   `<mission>/approvals/journal/` before registering it with Session. It returns
   exact standalone `mission-decision approve|reject|replan <uuid> <revision>
   <plan-sha256> <review-snapshot-sha256> <challenge-id>` prompts.
3. The host `UserPromptSubmit` hook retains immutable raw bytes, source-event
   identity and the Session-owned receipt. Generic reactions, actor assertions
   and assistant/tool events cannot authorize.
4. `accept_decision` holds the Mission publication lock while validating the
   receipt, raw prompt, roots and current reviews, then appends the disposition
   and consumes the Session challenge. `pending` exposes interrupted work;
   exact replay is idempotent and divergent decisions conflict.
5. Mission-linked activation and resume through `WorkspaceLifecycle` hold the
   same gate through the Ticket transition. Dispatch callers use
   `MissionApproval::guard_entry`. Each entry revalidates the journal chain,
   Session challenge and consumption. Finished Sessions, failed reviews,
   reject/replan and semantic changes refuse entry. Evidence-only appends retain
   semantic approval; edit/revert does not revive an old grant.

Independent Tickets retain ordinary Ticket authority. Unreadable Mission
authority fails closed. Fixture hook stdin validates plumbing only; it is not
proof that a human authorized execution. Real installed-host validation belongs
to the subsequent pilot.

Ticket prerequisites remain authoritative even with a valid grant: the Ticket
domain refuses a consumer advancing past an unresolved `depends_on` provider.
Mission link publication and Ticket entry share a per-Ticket lock. Lock order
is Mission typed locks first, then ascending Ticket link locks; entry
rediscovers links while holding that link lock and keeps it through the Ticket
update. A moving link set fails closed rather than authorizing against an
outdated discovery.

An unconsumed grant interrupted before a semantic edit remains visible as
pending history; reconciliation cannot consume it after that edit. Re-review
and issue a fresh challenge. Finished predecessor Sessions likewise require
a fresh challenge for the active successor.

Native progress and typed publication share the writer boundary: newly entering
`in-progress` needs an `EntryAuthority` capability minted by
`ApprovalTransaction::record_progress`. That method is trusted local library
API, not an exposed CLI/MCP operation; callers must use the facade's validated
`EntryGuard`. Completed/progress/result writes receive explicit nonsemantic
ledger revisions only while `plan_digest` is unchanged. Corrupt classification
fails closed; edit/revert remains semantic and never revives a grant.

Before locks, reads or writes, store-path checks reject symlinks and
non-directories between the workspace trust anchor and the managed Mission
store. This is path validation, not protection against concurrent malicious
filesystem replacement.

### Validation

From the workflow-tools workspace:

```sh
cargo test -p mission --features cli,mcp
cargo test -p mission-api
cargo check -p mission --features cli,mcp
cargo fmt --check -p mission -p mission-api
```

[Direct transport regressions](crates/mission/tests/direct_transports.rs)
launch both real executables against isolated temporary workspaces and clean
up the owned MCP processes.

## Dossier roadmap migration compatibility

`mission migrate-dossiers` imports authored dossier roadmaps only when they
contain the execution metadata required by the mission bundle validator. It
does not infer missing mode, scope, prompts, validation commands, or commit
checkpoints.

The canonical authored outcome heading is `## Outcome Summary`. Migration also
accepts the legacy-compatible `## Outcome` heading. Waypoints must use
`### W<n>. <title>` and include the required waypoint table fields, including
`Status`, `Scope`, `Mode`, `Session package`, `Part`, `Prompt`, `Validate`, and
`Commit checkpoint`.

Canonical authored mode labels are:

- `Execution side effect`
- `Dependent read-only waypoint` (stored as the mission `research` mode)

The canonical dependency field is `Depends`; omit it when a waypoint has no
prerequisites. Migration also reads the historical `Depends on` spelling.
Because dependency extraction recognizes only `W<n>` references, either
`none` or an em dash (`—`) in either spelling is treated as no dependencies.
Those values are compatibility input, not canonical authored syntax.

A read-only or no-change waypoint may use the canonical nonempty checkpoint
form `none (<reason>)`. It is preserved as the checkpoint message; an absent
or empty checkpoint remains a migration error.

The old index-table format (for example, a `| Waypoint | Package | Depends on |
Status |` table without per-waypoint sections) cannot represent all required
mission metadata. Migration blocks it with a re-author-and-re-review
diagnostic; rewrite and re-review it as the required per-waypoint schema
before migration rather than expecting the importer to synthesize fields.
