# Raspberry Pi deployment

Use 64-bit Linux and build on the Pi with current stable Rust and C build tools.
The default build bundles SQLite; a macOS executable will not run on the Pi.
Nothing here installs a service or changes SSH credentials.

## Build and initialize

After pushing the MCP changes and pulling them into your Pi checkout:

```sh
cargo build --locked --release
./target/release/dreamer --config fixtures/demo.toml db init
./target/release/dreamer --config fixtures/demo.toml lesson add --file fixtures/sample-lesson.json
./target/release/dreamer --config fixtures/demo.toml lesson search --query pagination
```

This creates an isolated synthetic demo database. `db init` applies migrations;
MCP startup requires an existing current schema and will not migrate it.

For real knowledge, keep config/data outside the checkout, use an absolute
`data_dir` and configure the chosen repository scope. Back up before migrations.
A stable binary path such as `/opt/dreamer/bin/dreamer` can be used later; no such
installation is performed by this project.

## Pi and Mac split

SQLite and stdio MCP are implemented. The Pi owns its local database. Clients
launch on-demand Dreamer processes on the Pi through SSH; no always-on daemon or
HTTP port is required. The Mac already has private-repository access and will
collect/refine knowledge through a future Mac component, submitting candidates
through the implemented write tool. The Pi needs no GitHub credential.

Do not synchronize the SQLite file via SMB/NFS or copy it between machines.
Clients call MCP operations against the Pi's local database. Collection,
scheduling, model refinement and status-promotion workflows remain pending.

## MCP over SSH

Use the absolute binary/config paths in [examples/codex.pi.toml](../examples/codex.pi.toml).
The client runs the equivalent of:

```sh
ssh -T -o BatchMode=yes your-user@your-pi /absolute/path/to/dreamer --config /absolute/path/to/dreamer.toml mcp
```

A bare invocation waits for an MCP client on stdin. Use `mcp --allow-writes` only
for a process that should expose `submit_candidate`. The default tools are
`search_lessons` and `get_lesson`. See [tool contracts and setup](mcp.md).

SSH key/agent authentication and a verified host key must already work from the
client's execution environment. Shell startup output must not contaminate stdout.
Keep diagnostics on stderr and use `ssh -T` without a pseudo-terminal. Verify
server discovery, search, and (when enabled) candidate submission in each client.
An authenticated connection in one client does not register the server elsewhere.

The write flag controls process capabilities; SSH/OS permissions control who can
launch it or directly modify the database. An HTTP service with separate client
identities/authentication is a later design. No HTTP listener or systemd unit is
included in this version.

## Validation and backups

Local tests cover SQLite persistence and real stdio MCP exchanges. Native Pi
linking/runtime and the live SSH/client connection must be verified on the Pi.
Back up SQLite consistently through its backup facilities rather than copying a
live main database without its WAL. Test restoration, retention and reboot behavior.

For system SQLite, install its development libraries and build with
`--no-default-features`. An ARM Rust type check on a Mac without a Linux C cross
compiler is:

```sh
cargo check --locked --no-default-features --target aarch64-unknown-linux-gnu
```

This skips compiling bundled C and does not link a Linux executable. Build the
default bundled configuration natively on the Pi for deployment.
