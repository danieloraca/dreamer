# Dreamer

A small Rust foundation for a Pi-hosted experience library for coding agents.
The goal is to learn reusable, evidence-backed lessons from merged pull requests
and let agents retrieve relevant lessons through MCP.

**This is a skeleton.** Configuration validation, domain types and extension
interfaces work. GitHub ingestion, model calls, persistence, search, scheduling
and the MCP protocol are not implemented. No data is collected or sent anywhere.

## Run

Install current stable Rust. Build and run from this directory:

```sh
cp dreamer.example.toml dreamer.toml
cargo run --locked -- status
cargo run --locked -- --help
```

`status` reads and validates the configuration without writing data or contacting
GitHub. Relative data paths resolve against the config file's directory.
The reserved `ingest`, `consolidate` and `mcp` commands print an explicit
not-implemented error and exit with code 2. Bad configuration exits with code 1.

```sh
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release
```

Commit `Cargo.lock` for reproducible application builds. No license has been
selected yet. `publish = false` prevents accidental crates.io publication.

## Layout

```text
src/config.rs          Strict TOML config validation
src/lesson.rs          Lesson lifecycle, applicability and source evidence
src/ingest/github.rs   Read-only PR source interface (no adapter)
src/consolidation.rs   Candidate extraction interface (no model integration)
src/store.rs           Repository-scoped storage/search interface (no database)
src/mcp.rs             Planned tool names (no MCP server)
src/main.rs            Status command and explicit placeholders
tests/skeleton.rs     Configuration, lesson and CLI behavior
docs/architecture.md   Boundaries and next steps
docs/pi.md             Deployment outline
```

Start with a small merged-PR sample and evaluate whether lessons improve a later
review before adding embeddings, a knowledge graph or speculative “dreaming.”
See [architecture](docs/architecture.md) and the [Pi outline](docs/pi.md).
# dreamer
# dreamer
