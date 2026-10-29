# Documentation workflow — source-to-fixture map

Source index: [README.md](../../../../../../../../transcripts/03-09-2026_doc-comment-standardization/README.md)  
Source route: [ROADMAP.md](../../../../../../../../transcripts/03-09-2026_doc-comment-standardization/ROADMAP.md)

The source defines four phases; all four are represented once, in source order.
The map preserves the original review boundaries and adds explicitly
fixture-authored expected/alternative outcomes for Mission's outcome-plan
schema.

| Source Part | Fixture waypoint | Preserved substance and adaptation |
|---|---|---|
| [01-doc-comment-contract.md](../../../../../../../../transcripts/03-09-2026_doc-comment-standardization/01-doc-comment-contract.md) | `doc-contract` | Required purpose, behavior/responsibility, intent, relationships, constraints, concise structure, item kinds, exceptions; no workspace-wide documentation or refactoring. |
| [02-documentation-health-check.md](../../../../../../../../transcripts/03-09-2026_doc-comment-standardization/02-documentation-health-check.md) | `health-check` | Inventory and stable status for missing, malformed, vague, overlong, unsupported, or intentionally undocumented items; no silent source rewrite. |
| [03-code-map-extraction.md](../../../../../../../../transcripts/03-09-2026_doc-comment-standardization/03-code-map-extraction.md) | `code-map` | Rustdoc JSON evaluation, element identity, module/path, description, signatures, per-module maps, aggregate index, and explicit extraction gaps; no deduplication. |
| [04-semantic-duplicate-analysis.md](../../../../../../../../transcripts/03-09-2026_doc-comment-standardization/04-semantic-duplicate-analysis.md) | `semantic-review` | Evidence-linked exact/related/unrelated candidates and missing abstractions, with human confirmation and refactoring retained; no automatic code mutation. |

The generated documentation example's operator result is a synthetic
`passed` record written through Mission publication and linked to an isolated
synthetic Test execution. It does not claim that the source workspace was
scanned or that any source validation command ran. Stable `doc-*` IDs and all
entity references are fixture-only.
