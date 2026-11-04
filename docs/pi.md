# Raspberry Pi deployment outline

Nothing in this project installs services or modifies a Pi. Use a 64-bit Linux
installation and build on the Pi initially; a macOS binary will not run there.

## What can run today

After creating your own GitHub repository and pushing this project, on the Pi:

```sh
git clone <your-repository-url> dreamer
cd dreamer
cargo build --locked --release
./target/release/dreamer --config fixtures/demo.toml db init
./target/release/dreamer --config fixtures/demo.toml lesson add --file fixtures/sample-lesson.json
./target/release/dreamer --config fixtures/demo.toml lesson search --query pagination
```

Install Git, current stable Rust and a C compiler/build tools separately if needed.
The default build bundles SQLite. The example creates a local demo database and
exits; it installs no service. `status` still only validates configuration.
Native Pi linking and runtime validation remain to be done. See the README's
check commands; ARM compile checks alone do not validate an installed Pi service.

## Intended Pi and Mac split

SQLite storage is ready for local development. The Pi will eventually own the
canonical database and MCP/service process. Dan's Mac already has access to the
private repositories; a future Mac component will collect and refine knowledge,
then read/write it through the Pi's MCP/service boundary. Do not add GitHub
credentials to the Pi. Neither the Mac component nor that network boundary exists.

Keep SQLite on the Pi's local filesystem; do not share the database file through
SMB/NFS or copy it back and forth as a synchronization mechanism. Future clients
will call service operations that use the same storage port as today's CLI.

## Once a service exists

Use a dedicated, unprivileged `dreamer` account. Keep the release binary at a
stable path such as `/opt/dreamer/bin/dreamer`, configuration under
`/etc/dreamer/`, and persistent data under `/var/lib/dreamer/`. Set an absolute
`data_dir` in the installed configuration. Keep credentials in a separate,
access-restricted file or systemd credentials, not in the repository.

A future systemd service may host authenticated MCP/API access; scheduling the
collector/refiner belongs on the Mac. Restrict Pi service writes to the data
directory, use `NoNewPrivileges=true`, `PrivateTmp=true` and protect the filesystem.
No unit is supplied because no server command works yet. Local SQLite migrations
should be a deliberate deployment operation, with a tested backup beforehand.

## Once MCP exists

Start with an on-demand stdio server. A client could launch it over authenticated
SSH using `ssh -T <pi-host> /opt/dreamer/bin/dreamer mcp` once that command is
implemented. There must be no banners or log messages on protocol stdout.
The MCP process will own knowledge writes from the Mac and reads for agents.
Test concurrent calls, authorization and retry behavior. Configure and verify
each client separately; Codex access
does not establish access for another client or ChatGPT session.

An always-on HTTP MCP service is a later option requiring authentication and a
deliberate network boundary. Do not port-forward an unauthenticated listener.

Before regular use: test restore from backups, data retention and Pi restart
behavior. Back up SQLite consistently (including its WAL through SQLite's backup
facilities) rather than copying a live main database file alone.

For development without bundled SQLite, install SQLite development libraries and
use `cargo build --locked --no-default-features`. For an ARM Rust type check on a
Mac without a Linux C cross compiler:

```sh
cargo check --locked --no-default-features --target aarch64-unknown-linux-gnu
```

This deliberately skips compiling bundled C and does not link a Linux executable.
Build the default bundled configuration natively on the Pi for deployment.
