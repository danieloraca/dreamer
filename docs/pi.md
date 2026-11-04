# Always-on Pi service

Dreamer now supports an authenticated Streamable HTTP MCP endpoint, supervised by
systemd. This mirrors SymNotes' `make deploy`, `restart`, `status` and `logs`
workflow, with a native Rust binary and SQLite. SymNotes continues using port
3444; Dreamer uses port 3445. No Docker, Pi GitHub token or Mac collector is needed
to run the knowledge service.

## First installation

After pushing these changes and pulling them on the Pi:

```sh
cd /home/danutz/Development/dreamer
make install-service
make status
```

Use 64-bit systemd Linux with current stable Rust, C build tools, make, Python 3.9+
and sudo. The installer requests sudo for system configuration. It:

- Builds normally, then installs `/usr/local/bin/dreamer`.
- Creates a dedicated, unprivileged `dreamer` user and `/var/lib/dreamer`.
- Creates `/etc/dreamer/dreamer.toml` from the Pi template if missing.
- Generates distinct read/write tokens once in `/etc/dreamer/dreamer.env`, owned
  by root with mode 0600. Secret values are never printed.
- Stops an existing service, backs up its database consistently through SQLite's
  backup API to `/var/backups/dreamer`, and applies pending migrations as `dreamer`.
- Installs/verifies the unit, enables it at boot, starts it and checks authenticated
  `/health`. systemd restarts the process after failures.

Reinstallation preserves existing configuration, tokens and knowledge. An unmanaged
unit causes a preflight failure; use `sudo systemctl edit dreamer` for local unit
customization. The prior binary is retained as `/usr/local/bin/dreamer.previous`.
Deployment is not transactional: if a later step fails, earlier changes/backups
remain. Inspect the reported error and service logs before retrying or restoring.

The installer manages only `/var/lib/dreamer`; it refuses an installed config
pointing elsewhere. Existing checkout/demo databases are preserved and are not
copied into the new service automatically. If you already hold real knowledge in
another database, plan a SQLite-consistent migration before switching clients.

## Access and tokens

With the template's current LAN settings:

```text
http://192.168.0.25:3445/mcp
```

The template also allows the Pi's Tailscale address from SymNotes' instructions:
`http://100.106.171.95:3445/mcp`. Review `[http].allowed_hosts` if addresses change.
The native default listens on localhost; the Pi template deliberately binds all
IPv4 interfaces to serve the existing private LAN/Tailscale network. Use private
network access; put TLS in front before using an untrusted network.

Every MCP and health request requires `Authorization: Bearer <token>`:

| Environment variable | Access |
| --- | --- |
| `DREAMER_MCP_TOKEN` | Search/get and lifecycle history |
| `DREAMER_MCP_WRITE_TOKEN` | Reads, candidate submission and evidence-backed lifecycle changes |

Read the credentials locally with `sudoedit /etc/dreamer/dreamer.env` and transfer
only the required token to each client through its private configuration. Do not
commit or paste credentials into chats. Set the read token in the environment
that launches Codex and use [examples/codex.http.toml](../examples/codex.http.toml).
The Mac agent tools use the write token at the same URL. Browser Origin
requests to MCP are rejected. The [dashboard](dashboard.md) uses a separate
read-only browser API with the read token; there is no OAuth flow. Read and write legacy sessions are separated.

## Updates and operation

```sh
# After pulling a reviewed update:
make deploy
make restart
make status
make logs
```

`make deploy` repeats the installer safely with existing config/tokens. Verify
backups before code/schema updates and test restores. Service logs go to journald;
credentials do not appear in the unit's command line or startup log. The service
handles SIGTERM for shutdown/restart. Stop it with `sudo systemctl stop dreamer`;
use `sudo systemctl disable --now dreamer` to disable boot startup.

Local development can still use stdio/SSH. For foreground HTTP, initialize a local
configuration's database, supply the token environment, then run:

```sh
./target/release/dreamer --config dreamer.toml serve
# With a distinct write token also set:
./target/release/dreamer --config dreamer.toml serve --allow-writes
```

## Verification limits

Tests exercise real HTTP MCP clients, bearer roles, Origin/Host checks, payload
limits, persistence and SIGTERM shutdown on macOS. Three simulated installer tests
check config/token preservation, backups and preflight refusals without sudo or
system changes. Installer syntax and make recipes are also checked locally;
installing/verifying systemd and native Pi runtime
must be checked on the Pi. ARM Rust type checks use:

```sh
cargo check --locked --no-default-features --target aarch64-unknown-linux-gnu
```

That does not link a Linux executable. Build the default bundled SQLite version
natively on the Pi, as `make install-service` does.


## Lesson lifecycle upgrade

[Lifecycle tools](lifecycle.md) require schema 3 and the new Pi API. After pulling
the update, `make deploy` backs up the existing database before migration. It
preserves configuration, both tokens and original lesson/evidence records. Older
binaries cannot read schema 3; a rollback requires the matching pre-upgrade binary
and consistent backup, not just `dreamer.previous`. Keep the newer DB separately
and stop the service before restoring. The Mac can still recall from an older Pi,
but reconciliation reports that deployment is required. Scheduled backup/restore
automation is a later milestone in the [master plan](master-plan.md).


## Retrieval upgrade

The current update also applies schema 4 for optional lesson component/path scope.
`make deploy` backs up before applying every pending migration. Old lessons and
lifecycle history are preserved. Older Mac clients can still use the literal
search API; updated Mac clients fall back on an older Pi for recall, but scoped
candidate publication requires the new Pi API. See [retrieval](retrieval.md).
