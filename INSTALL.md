# Install Mission

Install a Rust toolchain supporting edition 2024, Cargo, Git and a native C/C++
build toolchain for the SQLite and Git dependencies.

```bash
git clone https://github.com/mankinskin/mission
cd mission
cargo build --locked -p mission --features cli,mcp
cargo run --locked -p mission --features cli --bin mission -- --help
```

To install both binaries into Cargo's configured binary directory:

```bash
cargo install --locked --path crates/mission --features cli,mcp
mission --help
```

`mission-mcp` is a stdio MCP server. Configure its consumer workspace explicitly
as described in [README.md](README.md#named-mission-native-mcp-tools).
Libraries are available in `crates/mission` and `crates/mission-api`.
Git dependencies are resolved from the owning domains' `main` branches and
pinned by this repository's lockfile.
