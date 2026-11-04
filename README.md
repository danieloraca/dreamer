# Dreamer

A small Rust foundation for a Pi-hosted experience library for coding agents.
The goal is to learn reusable, evidence-backed lessons from merged pull requests
and let agents retrieve relevant lessons through MCP.

**Knowledge storage and stdio MCP work.** SQLite migrations, candidate import,
retrieval, repository-scoped search and an MCP server are implemented. GitHub
ingestion, a Mac collector, model refinement, scheduling and installed Pi deployment
are pending. Dreamer does not call GitHub or models, or open a network listener.

The intended split is a Pi-hosted library/service and a Mac-side collector that
uses the Mac's existing private-repository access. The Mac will submit/refine
knowledge through the Pi's MCP boundary; the Pi does not need GitHub credentials.
Clients can launch the server on the Pi through SSH; see [MCP setup](docs/mcp.md).

## Run

Install current stable Rust. Build and run from this directory:

```sh
cp dreamer.example.toml dreamer.toml
cargo run --locked -- status
cargo run --locked -- --help
```

`status` reads and validates the configuration without writing data or contacting
GitHub. Relative data paths resolve against the config file's directory.
The reserved `ingest` and `consolidate` commands print an explicit
not-implemented error and exit with code 2. Operational errors exit with code 1;
CLI usage errors exit with code 2.

## Try the local fixture

The fixture is invented and uses `example.invalid` links. Its separate config
keeps demo records out of the Gecko repository scope:

```sh
cargo run --locked -- --config fixtures/demo.toml db init
cargo run --locked -- --config fixtures/demo.toml lesson add --file fixtures/sample-lesson.json
cargo run --locked -- --config fixtures/demo.toml lesson search --query pagination
cargo run --locked -- --config fixtures/demo.toml lesson get --id pagination-scope-001
```

This creates `data/demo/knowledge.sqlite3` (ignored by Git). `db init` applies two
versioned migrations; `lesson add` also initializes/migrates as needed. Identical
imports return `Unchanged`; a conflicting ID or PR revision fails atomically.
New imports must be candidates and match the configured repository exactly.
There is no approval, update, deletion or refinement workflow yet.

Search/get open an existing, current database read-only and return JSON including
status, evidence, producer/method and a SQLite-assigned UTC storage timestamp.
Search matches a literal substring of summary/applicability, folds ASCII case,
excludes deprecated lessons and accepts a limit of 1–100 (config default: 5).
Results are ordered by storage time descending, then ID ascending. Search is a
bounded scan within the repository, not semantic search or relevance ranking.

`github.repository` currently supplies local repository scope for compatibility
with the original config; it grants no GitHub access. See the fixture JSON for
the complete candidate input format. Producer, URLs and descriptions are claims
to inspect, not authenticated identity or verified proof.

## MCP

Initialize the database explicitly, then launch the read-only stdio server:

```sh
cargo run --locked -- --config fixtures/demo.toml db init
cargo run --locked -- --config fixtures/demo.toml mcp
```

The server waits for an MCP client on stdin. Stdout contains protocol messages
only; errors go to stderr. MCP requires a current schema and never creates or
migrates the database. Migrate deliberately with `db init` before launching.

| Tool | Arguments | Availability |
| --- | --- | --- |
| `search_lessons` | `repository`, `query`, optional `limit` | Default |
| `get_lesson` | `repository`, `id` | Default |
| `submit_candidate` | `lesson` object matching the fixture | `mcp --allow-writes` |

Every tool enforces repository scope. Reads return structured JSON and matching
text JSON. Submission uses the same candidate-only, atomic, idempotent store as
the CLI. It cannot replace, verify or delete lessons. The write flag controls one
process; SSH/OS permissions determine who can start it. There is no HTTP server.
See [client configuration](docs/mcp.md) and the [Pi outline](docs/pi.md).

## Validate

The default build compiles a bundled SQLite, requiring a C compiler as well as
Rust. No separately installed SQLite is needed. An optional
`--no-default-features` build links against system SQLite instead.

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
src/store.rs           Repository-scoped storage/search interface
src/store/sqlite.rs    Atomic SQLite candidate imports, reads and search
migrations/           Embedded, append-only SQL migrations and a version ledger
fixtures/             Synthetic PR lesson and isolated demo configuration
src/mcp.rs             Stdio MCP server and opt-in candidate submission
src/main.rs            Local CLI and MCP process startup
tests/                Configuration, persistence, CLI and real stdio MCP checks
examples/             Credential-free local/Pi Codex configuration examples
docs/mcp.md           Tool contracts, client setup and SSH access
docs/architecture.md   Boundaries and next steps
docs/pi.md             Deployment outline
```

Start with a small merged-PR sample and evaluate whether lessons improve a later
review before adding embeddings, a knowledge graph or speculative “dreaming.”
See [architecture](docs/architecture.md) and the [Pi outline](docs/pi.md).
