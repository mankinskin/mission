# Requester intent authority (schema v4)

`mission_intent` preserves success requirements independently of editable plan
objectives. Its strict `operation.action` accepts `prepare_baseline`,
`confirm_baseline`, `prepare_amendment`, `append_amendment`, or `get`.
All calls identify `workspace_root` and `mission_id`; mutations also require
`expected_revision`. Preparation validates the proposed source spans and
returns a Session confirmation challenge and exact requester prompt.
Only the trusted host's captured confirmation can authorize persistence.
Callers pass `reference: {session_id, challenge_id}`, never a raw receipt or
an approval flag. This confirmation is not execution approval.

Baseline requirements contain a stable `requirement_id`, exact UTF-8 byte
`source` span and excerpt, final `outcome`, `category`, and compatible
`evidence_policy`. Categories are `working_behavior`, `artifact`, `decision`,
and `research_review`. Amendments require a fresh confirmation, a unique
`amendment_id`, current `prior_chain_sha256`, requester `source`, and explicit
`changes` (`exclude`, `replace`, or `add`). Replacements and exclusions name
the prior requirement ID; IDs cannot be reused.

The CLI accepts the same action JSON:

```sh
mission --workspace /absolute/workspace intent <mission-uuid> '{"action":"get"}'
```

The generic MCP equivalent is `mission_operation` with `operation: "intent"`,
`mission_id`, and `intent_action`. All three transports share the same provider.
Confirmed records live in `intent/baseline.json` and append-only numbered
`intent/amendments/` records. Legacy v2/v3 sources are not rewritten;
readback reports absent intent until explicitly confirmed. Baseline and
amendment hashes use domain-separated compact JSON; source hashes cover exact
bytes. The effective chain binds the original baseline and every receipt-bound
amendment. Corrupt records, stale revisions, incompatible evidence, invalid
spans, and conflicting replays return explicit errors.
