# Contributing

Mission owns the code under `crates/mission-api` and `crates/mission`, plus its
domain documentation. Keep transport behavior aligned through the facade.
Consumer stores and shared workflow guidance remain with their owning
repositories.

Run standalone validation from this repository root:

```bash
cargo fmt --all -- --check
cargo check --locked -p mission --features cli --bin mission
cargo check --locked -p mission --features mcp --bin mission-mcp
cargo test --locked --workspace --features cli,mcp,test-evidence-provider
```

The test-evidence provider is for disposable fixtures only and is not enabled
by production CLI/MCP builds.

When developing in the initialized meta-workspace, also run the existing
Session capture-hook, guidance/installer and cross-domain integration checks
from `workflow-tools`:

```bash
cargo test --locked -p mission -p mission-api \
  --features cli,mcp,test-evidence-provider,meta-workspace-tests
```

Do not enable `meta-workspace-tests` in a standalone checkout: those tests
intentionally require the real Session capture-hook, workspace guidance,
installer and peer domains.
Keep regression tests focused on changed behavior and document validation in
the pull request. Commit and publish Mission changes before updating a
consumer's submodule pointer.
