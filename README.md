# Mission

## Direct authoring and operator evidence

The `mission` package provides the `mission` binary with feature `cli` and the
`mission-mcp` stdio server with feature `mcp`. Both transports use the same
[facade operations](crates/mission/src/lib.rs). No dossier, migration, or bundle
file is required for direct authoring.

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
The projection renderer is `mission-api/2`; older generated output reports drift
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
