# Activity, backups and recovery

The Pi holds knowledge; the Mac performs on-demand PR learning. Neither the Pi
nor the dashboard polls GitHub or runs a model. This update adds schema 5, daily
checked copies and bounded learning reports. Pull and run `make deploy` on the Pi;
start a new Codex task after updating the Mac client. No new credential is needed.

## Backup operation

The installer enables `dreamer-backup.timer` (daily, up to 15 minutes jitter,
persistent after downtime). Each run is a short-lived Python process as `dreamer`,
with low CPU/I/O priority, no network and no model calls. It uses SQLite's online
backup API, which includes committed WAL data; copying only a live `.sqlite3` file
is insufficient. See SQLite's [backup](https://www.sqlite.org/backup.html) and
[WAL](https://www.sqlite.org/wal.html) documentation.

```sh
make backup                 # run a copy now; waits for completion
make backup-status          # timer and last service result
sudo journalctl -u dreamer-backup.service -n 30 --no-pager
```

`/etc/dreamer/backup.json` configures `directory`, `keep` (default 14, 1–3650)
and `timeout_seconds` (default/max 120 for the installed service). The destination
must be under `/var/lib/dreamer`, a real private directory owned by `dreamer`.
Reinstalling preserves this config, repository settings and both tokens. The
service deadline is 180 seconds, including checking/pruning; a very large database
may need an operator-managed service override and an explicit capacity review.

Scheduled snapshots live in `/var/lib/dreamer/backups`, mode 0600, alongside a JSON
manifest containing schema version, size, SHA-256 and completion time. Each copy
passes SQLite quick/foreign-key checks and contiguous migration checks; only then
are old recognized scheduled pairs pruned. Operator files and root-owned pre-deploy
copies in `/var/backups/dreamer` are untouched. Failed copies preserve the previous
success report and do not prune. An interrupted process can leave a temporary or
unpaired file; inspect it locally rather than assuming it is a checked backup.

`backup-status.json` is fixed, redacted metadata next to the live database. The
dashboard shows the last reported attempt, last checked success and whether that
success is over 48 hours old. A missing report or an interrupted process does not
prove that a backup is running. Inspect systemd when the report is stale.
Checksums detect accidental corruption; they are not signatures or encryption.

These copies protect against application mistakes. **Copy checked snapshot and
manifest pairs off the Pi** to survive disk loss; no off-device destination is
configured automatically. Verify the pair after transfer. Do not expose the backup
directory through HTTP or copy active WAL files as an alternative to a snapshot.

## Restore into a new file first

Run this with a scheduled snapshot's **absolute path**, as `dreamer`. Replace the
example filename locally; no token is required. Use a matching-schema Dreamer
binary, retaining the matching release with off-device backups.

```sh
# On the Pi; choose a NEW, unused recovery directory.
sudo install -d -m 700 -o dreamer -g dreamer /var/lib/dreamer-recovery
sudo -u dreamer /usr/bin/python3 /usr/local/lib/dreamer/backup.py verify \
  --snapshot /var/lib/dreamer/backups/scheduled-EXAMPLE.sqlite3
sudo -u dreamer /usr/bin/python3 /usr/local/lib/dreamer/backup.py restore \
  --snapshot /var/lib/dreamer/backups/scheduled-EXAMPLE.sqlite3 \
  --output /var/lib/dreamer-recovery/knowledge.sqlite3
sudo -u dreamer /usr/local/bin/dreamer db check \
  --file /var/lib/dreamer-recovery/knowledge.sqlite3
```

`restore` refuses an existing output (including a dangling symlink), verifies the
manifest and checks the new file's integrity, schema and logical contents. It does
not stop services or replace live knowledge. `db check --file` requires no service
config and never creates/migrates a database; `--allow-older` can inspect an older
known schema, but does not make an older binary compatible with newer data. Older
pre-deploy copies have no scheduled manifest: preserve them and use a separately
reviewed matching-binary recovery procedure, rather than inventing a manifest.

Before switching, inspect several lessons/history in the recovery copy with the
matching binary and a private temporary config pointing to that directory. Finish
all writers, including stdio clients; then stop the main service and backup timer:

```sh
sudo systemctl stop dreamer-backup.timer dreamer-backup.service dreamer.service
# Both destination names must be unused. Preserve the ENTIRE old directory,
# including WAL/SHM and backups, for rollback. Never overwrite individual live files.
sudo mv -T /var/lib/dreamer /var/lib/dreamer-before-recovery
sudo mv -T /var/lib/dreamer-recovery /var/lib/dreamer
sudo -u dreamer /usr/local/bin/dreamer --config /etc/dreamer/dreamer.toml db init
sudo systemctl start dreamer.service
make status
# Check authenticated MCP reads/dashboard before enabling scheduled copies again.
sudo systemctl start dreamer-backup.timer
```

Those are deliberate admin actions, not an automatic installer restore. If startup
or reads fail, stop the same services, move the failed recovered directory aside,
then move `/var/lib/dreamer-before-recovery` back and restart with its matching
binary. Preserve both directories until validated. Root-owned backups must first
be copied, with their manifests when present, into private staging readable by the
service account. Do not relax permissions on live data to make recovery easier.

## Learning reports

Updated Mac MCP **and CLI** capture/publication/reconciliation entry points write
private operational records in `pr-cache/runs` (at most 200). After learning, a
best-effort flush delivers at most five pending records with a three-second total
deadline. Later learning operations retry a backlog; an unavailable or older Pi
does not turn a successful learning operation into a failure. Diagnostics:

```sh
~/.local/share/dreamer/dreamer mac --settings ~/.config/dreamer/mac.toml activity
~/.local/share/dreamer/dreamer mac --settings ~/.config/dreamer/mac.toml activity --sync
```

Use your installed settings path if different. Read-only recall/get tools do not
emit learning reports or flush the outbox. Success responses add
`activity_reporting: reported|queued|unavailable`. Failed business operations retain
their original error; the report includes no exception text. Local terminal updates
and acknowledgements use an OS lock; stale started reports cannot overwrite a
completed Pi record.

The Pi exposes read-only `get_learning_runs` and writer-only `record_learning_run`.
It retains 500 reports per repository: phase, state, PR/revision, client times,
server receipt times and bounded counts. No raw prompts, tool arguments, source
bodies or credentials are stored. Started records mean **completion not reported**,
not proof of an active process. Failed operations have unknown counts because
writes may have partially completed. Counts describe operation outcomes, not
verified lesson quality; a successful no-new-lessons publication is still learning.

Reports are operational visibility, not a complete audit: bounded offline queues
can age out, malformed/unscoped calls cannot reliably be attributed, and a process
can exit before reporting. Server receipt time can be later than the actual Mac
operation. Existing knowledge is not retroactively given synthetic activity.
