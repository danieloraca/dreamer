# Raspberry Pi deployment outline

Nothing in this project installs services or modifies a Pi. Use a 64-bit Linux
installation and build on the Pi initially; a macOS binary will not run there.

## What can run today

After creating your own GitHub repository and pushing this project, on the Pi:

```sh
git clone <your-repository-url> dreamer
cd dreamer
cargo build --locked --release
cp dreamer.example.toml dreamer.toml
./target/release/dreamer status --config dreamer.toml
```

Install Git and stable Rust separately if needed. `status` validates configuration
and exits; it is not a daemon. No database or service is created. The project has
been tested on macOS and compile-checked for `aarch64-unknown-linux-gnu`;
native Pi build/runtime validation remains to be done.

## Once ingestion and storage exist

Use a dedicated, unprivileged `dreamer` account. Keep the release binary at a
stable path such as `/opt/dreamer/bin/dreamer`, configuration under
`/etc/dreamer/`, and persistent data under `/var/lib/dreamer/`. Set an absolute
`data_dir` in the installed configuration. Keep credentials in a separate,
access-restricted file or systemd credentials, not in the repository.

A future systemd oneshot unit should run a single bounded ingestion/consolidation
pass and exit; a timer can invoke it periodically. Require non-overlapping runs,
retry/backoff, cursor persistence and clean error exits first. Restrict writes to
the data directory, use `NoNewPrivileges=true`, `PrivateTmp=true` and protect the
filesystem. No unit is supplied yet because the worker command does not work.

## Once MCP exists

Start with an on-demand stdio server. A client could launch it over authenticated
SSH using `ssh -T <pi-host> /opt/dreamer/bin/dreamer mcp` once that command is
implemented. There must be no banners or log messages on protocol stdout.
The MCP process and ingestion worker will share the library, so test concurrent
read/write behavior. Configure and verify each client separately; Codex access
does not establish access for another client or ChatGPT session.

An always-on HTTP MCP service is a later option requiring authentication and a
deliberate network boundary. Do not port-forward an unauthenticated listener.

Before regular use: test restore from backups, data retention and Pi restart
behavior. Back up SQLite consistently (including its WAL through SQLite's backup
facilities) rather than copying a live main database file alone.
