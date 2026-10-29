# Mission

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
| `source_schema` | `mission-bundle-v1`, `mission-source-v2`, or `ticket-unversioned`; never inferred/coerced from unknown versions. |
| `id` | Non-nil UUID: `AcceptedMission.mission_id`, `TypedDraft.mission_id`, or the existing `TicketManifest.id`. |
| `title` | Nonblank string: Mission title or Ticket's typed string `extra.title`. No normalization or domain write occurs. |

These are the only demonstrated common identity/typed-attribute semantics.
Native bundle schema `1`, typed Mission source `2`, and Part source `1` keep
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
Renderer `mission-api/4` identifies entity-driven waypoint relationships and
the existing escaping/canonical-reference output. Older generated bytes can
report drift; they are never rewritten by
these read-only APIs.

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
containing the README, roadmap and Parts. An absent external-status snapshot and
the empty snapshot have the same explicit empty representation, so typed
publication and subsequent verification agree byte-for-byte. Transport success
does not imply acceptance of historical review rows.
The projection renderer is `mission-api/3`; older generated output reports drift
until explicitly republished through the owning Mission operation.

`check-generated` reports `generated-changed`, `generated-missing`,
`generated-extra`, `generated-dangling` and `generated-destination-invalid`
separately. The latter includes redirected generated files or destinations
outside the owning workspace. Checks are read-only; regenerate through the
owning publication operation rather than editing projections.

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
