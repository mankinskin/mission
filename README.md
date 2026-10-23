# Mission

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
