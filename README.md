# Dreamer

A Pi-hosted knowledge library for coding agents. Store evidence-backed lessons
from pull requests and retrieve them through MCP.

Built with Rust and SQLite. Candidate imports, search, and HTTP/stdio MCP are
implemented. Manual GitHub collection produces local review drafts; automated
semantic refinement and publishing are next.

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

- [MCP tools and client configuration](docs/mcp.md)
- [Pi installation and operation](docs/pi.md)
- [Architecture and roadmap](docs/architecture.md)
- [Mac collection and preview workflow](docs/collector.md)
