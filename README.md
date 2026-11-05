# Dreamer

A Pi-hosted knowledge library for coding agents. Store evidence-backed lessons
from pull requests and retrieve them through MCP.

Built with Rust and SQLite. Candidate imports, search, and HTTP/stdio MCP are
implemented. Mac MCP tools let Codex recall prior knowledge and automatically
store source-backed lessons from the PRs it reads.

## Dashboard

After deploying, open `http://<pi-address>:3445/` and enter the **read token**.
Select a repository, search lessons and follow their source evidence.
No extra service or frontend build is needed. [Dashboard guide](docs/dashboard.md).

## Agent memory on Mac

Run `make install-mac`, then start a new Codex task. Codex recalls knowledge before
coding and stores supported lessons from PRs it reads, without per-lesson approval.
[Agent memory setup and tools](docs/agent-memory.md).
Configure additional Gecko repos with the [repository allowlist](docs/repositories.md).

## Mac collector

With GitHub CLI installed and signed in:

```sh
cargo run --locked -- collect --repository owner/repo --count 10 --output data/collections/first-run
```

This saves private evidence and unverified review prompts locally. Use `--holdout`
for a review comparison. See [collection and curation](docs/collector.md).

## Pi service

From the Pi checkout:

```sh
make install-service
make status
```

The service starts at boot, restarts after failures, and stores knowledge in
`/var/lib/dreamer`. Separate bearer tokens provide read and write access.
Update with `make deploy`; follow logs with `make logs`.

See [Pi setup](docs/pi.md) for prerequisites, credentials, backups and deployment.

## Local demo

```sh
cargo run --locked -- --config fixtures/demo.toml lesson add --file fixtures/sample-lesson.json
cargo run --locked -- --config fixtures/demo.toml lesson search --query pagination
```

This uses synthetic data in a separate, Git-ignored demo database.

## Development

```sh
make test
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

## Documentation

- [Activity, backups and recovery](docs/operations.md)
- [CI checks and local equivalents](docs/ci.md)
- [Context-aware retrieval](docs/retrieval.md)

- [Master plan and milestone status](docs/master-plan.md)
- [Correcting and consolidating lessons](docs/lifecycle.md)

- [MCP tools and client configuration](docs/mcp.md)
- [Pi installation and operation](docs/pi.md)
- [Architecture and roadmap](docs/architecture.md)
- [Mac collection and preview workflow](docs/collector.md)
