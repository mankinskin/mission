# W7. Migrate selected dossiers safely (`dossier-migration`)

<!-- mission: 00000000-0000-4000-8000-000000002001; revision: 3; renderer: mission-api/7; external-status-snapshot: ticket-status-v1:{} -->

Previous: [06-session-attachment.md](06-session-attachment.md) | Next: [08-guidance-retarget.md](08-guidance-retarget.md)

## Waypoint Details

| Field | Value |
|---|---|
| **Mode** | "execution-side-effect" |
| **Scope** | Single session |
| **Child mission** | None |
| **State** | pending |
| **Prerequisites** | session-attachment |
| **Session package** | dossier-migration |
| **Prompt** | Fixture-only adaptation: Enumerate source folders at runtime, import only the current roadmap, preserve provenance and current links, and report migrated/skipped/blocked inputs. Preserve the boundary: Leave original dossiers and version snapshots unchanged; derive IDs only by the approved title rule and defer unsupported legacy cases explicitly. |
| **Artifacts** |  |
| **Requirements** |  |
| **Non-goals** | Do not delete source dossiers, impose historical folder counts, or convert every legacy folder. |
| **Commit checkpoint** | Fixture review checkpoint: dossier-migration |

## Validation

- `Compare dry-run inventory to filesystem contents, inspect the migrated dossier record, and verify all excluded snapshot paths.`

## Part

## Adapted objective

Enumerate source folders at runtime, import only the current roadmap, preserve provenance and current links, and report migrated/skipped/blocked inputs.

## Constraints and validation

Leave original dossiers and version snapshots unchanged; derive IDs only by the approved title rule and defer unsupported legacy cases explicitly. Validation guidance: Compare dry-run inventory to filesystem contents, inspect the migrated dossier record, and verify all excluded snapshot paths.. Non-goal: Do not delete source dossiers, impose historical folder counts, or convert every legacy folder..


## Planned outcomes (manual guidance)

Descriptive only; not acceptance criteria, branching, progress, prerequisites, scheduling, dispatch, or automatic recovery. Actual operator results remain separate.

**Expected outcome:** Eligible current roadmaps are represented while blocked folders and excluded history remain visible and auditable.

**Alternative outcome 1:** A source folder is ambiguous, unsafe, or outside the approved migration boundary.

**Acceptable handling (manual options):**

- Record the condition as fixture-authored guidance, preserve all source bytes, and stop any affected follow-up.

