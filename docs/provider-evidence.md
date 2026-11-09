# Execution contracts and provider evidence

## Contract and authority

Native bundle v3 / typed source v4 declares one outcome separately from work
kind, rationale, authoritative context, task/path boundary, dependencies,
checkable pre/postconditions, acceptance/preflight/regression checks, evidence
classes, tools/permissions/read-write roots, environment/profile, finite failure
handling and producer/consumer handover. Omitted dependency, regression,
handover and runtime-provider routes require explicit reasons. Invalid IDs,
roots, condition/check links and dependencies fail validation.

Write roots are component-aware descendants of the declared task boundary.
Producer handover checks support postconditions; consumer checks support
preconditions and the consumer depends on the producer. Checks describe commands;
Mission never executes them or intercepts unrelated filesystem/tool access.

`waypoint_execution_contract_digest` is SHA-256 of compact ordered typed JSON.
Both scope reviews and approval/entry bind the contract set, confirmed source,
intent/amendments and review identities. Legacy bytes/history remain readable;
new legacy entry requires an authority upgrade, not implicit conversion.

## Read-only trust boundary

`ReadOnlyEvidenceProviderV4` reads its independently owned store/API. It returns
the current environment digest, finds a preflight receipt for the exact binding,
and resolves an opaque `ProviderEvidenceRefV4`. A receipt is not a file path.
`ResolvedProviderEvidenceV4` returns bytes plus an independent SHA-256; Mission
recomputes that hash before decoding `CompletionEvidenceV4` (schema version 4).

Bindings contain provider/version/route/receipt class, Mission/waypoint, exact
owning root/profile/environment, immutable source/confirmed intent-chain,
semantic plan/contract and prerequisite artifact digests. Provider records
declare readiness and explicit `current`, `stale` or `revoked` validity. Every
relevant entry rereads preflight and prerequisite evidence; there is no invented
retention timeout, artifact deletion or cached readiness flag.

`EvidenceAdmissionV4::default()` has **no production adapter**. Unknown or
unavailable production capability returns `capability_unavailable`; there is
no local command fallback. `test_only(Arc<dyn ReadOnlyEvidenceProviderV4>)` and
the facade's `with_test_evidence_provider` methods exist **only** with the
explicit opt-in `test-evidence-provider` Cargo feature. Default builds, including
ordinary `cli,mcp` builds, do not expose these constructors. The read-only
provider trait and evidence models remain public without that feature.

The feature enables typed, disposable-test injection, not a production adapter.
Even when compiled with it, production CLI/MCP dispatch always constructs the
default admission; no flag, environment variable, MCP field, path or receipt
prefix selects a fixture. A fixture store on disk does not make production ready.
This compile-time API boundary is not kernel/process isolation: embedding Rust
code that deliberately opts in can inject its own provider.
The future sandbox contract is a reference, not a delivered runtime dependency.

Runtime-provider routes require preflight before activation, resume, dispatch,
direct progress or linked-Ticket entry. Explicit no-runtime routes do not claim
sandbox readiness. Routes consuming declared handovers still reread their
producer evidence before advancing; every new completion requires a provider.

## Completion, handover and bounded diagnostics

Completion resolves the opaque result reference, in addition to requiring its
current plan evidence binding and native Ticket/child completion. All declared
postconditions and applicable non-preflight checks must be present exactly once.
Check digests hash compact JSON `["mission-automated-check/v4", typed_check]`,
binding ID, command, cwd, kind and evidence class. Zero execution, any failure,
any skipped required check, missing results or incompatible classes block.
Artifact outcomes may use deterministic document/schema/link/example checks.
Mocks are diagnostic-only unless independently confirmed intent explicitly
allows mock behavior; editable contract metadata cannot grant that permission.

Provider-owned attempt evidence must respect the declared failure trigger,
diagnostic action and finite bound: at most one diagnostic rerun, with a
nonblank justified fix; otherwise `failure_bound_exhausted`/block or escalation.
This is evidence admission, not a retry scheduler or alternative acceptance path.

`HandoverArtifactV4` contains bytes/hash, schema identity and provider schema
verdict, owning root, exact producer/consumer endpoints and both contract
digests. Missing, tampered, wrong-schema/root/consumer artifacts block completion
or consumer advance. Required precondition links are not inferred from filenames.
Prerequisites are reread, not copied from caller-provided hashes.

Accepted receipt hashes are pinned create-new under the publication lock at
`approvals/runtime-evidence/<receipt-identity-sha256>.sha256`. Identity hashes
compact JSON `["mission-provider-receipt/v4", provider, version, reference]`.
Only well-formed append-only pins are exempt from the live output BOM; archived
snapshots retain exact bytes. Changed bytes under an accepted opaque receipt
return `provider_replay_divergence`. Plan/source locks, control CAS, legal
nonsemantic revisions and stale/conflicting request rejection remain in force.

## Diagnostics and executable examples

- `provider_receipt_missing`, `provider_evidence_corrupt`: absent/forged receipt
  or content/hash mismatch.
- `provider_identity_mismatch`, `provider_version_incompatible`,
  `provider_root_mismatch`, `provider_profile_mismatch`,
  `provider_waypoint_mismatch`, `provider_digest_mismatch`: wrong binding.
- `provider_unready`, `provider_evidence_stale`, `provider_evidence_revoked`:
  current provider readback denies eligibility.
- `condition_evidence_required`, `automated_check_evidence_required`,
  `zero_executed_checks`, `automated_check_failed`, `check_metadata_mismatch`,
  `evidence_class_mismatch`, `mock_evidence_prohibited`: acceptance unmet.
- `handover_evidence_required`, `handover_contract_mismatch`,
  `handover_root_mismatch`, `handover_schema_invalid`,
  `handover_content_mismatch`: consumer cannot advance.
- `diagnostic_fix_required`, `failure_bound_exhausted`,
  `provider_replay_divergence`: block/escalate, never weaken acceptance.

[API regressions](../crates/mission-api/tests/provider_evidence.rs) and
[facade regressions](../crates/mission/tests/provider_evidence.rs) create real
`TempDir` stores, demonstrate accepted disposable evidence and exercise rejection
at actual lifecycle/transport guards. They establish no production isolation or
provider readiness. W8 can document these diagnostics/examples without installing
or promoting a fixture adapter.

## Validation features

From the workflow-tools workspace, run the fixture-bearing targets explicitly:

```sh
cargo test --locked -p mission-api --features test-evidence-provider -j1
cargo test --locked -p mission --features cli,mcp,test-evidence-provider -j1
cargo test --locked -p mission --features cli,mcp,test-evidence-provider --test named_mcp --test approval_native_routes -j1
cargo test --locked -p mission-api -p mission --doc -j1
cargo test --locked -p mission-api -p mission --features mission/cli,mission/mcp --doc -j1
cargo check --locked -p mission --lib
cargo check --locked -p mission --features cli,mcp
```

`mission-api/provider_evidence` and Mission's `provider_evidence`, `w5_reopen`,
`named_mcp`, and `approval_native_routes` integration targets require the test
feature. Cargo omits them from an unqualified default test run and rejects an
explicit target selection without its required features; such a run is **not**
complete W7 validation. The feature-enabled suite also tests production transport
rejection. Default and ordinary transport-feature doctests separately prove the
three fixture injection methods cannot compile.
