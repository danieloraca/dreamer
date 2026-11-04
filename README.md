# Dreamer

A Pi-hosted knowledge library for coding agents. Store evidence-backed lessons
from pull requests and retrieve them through MCP.

Built with Rust and SQLite. Candidate imports, search, and HTTP/stdio MCP are
implemented. The Mac collector and automated refinement are next.

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
