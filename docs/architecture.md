# Architecture and next steps

One crate keeps the initial project easy to navigate. The SQLite adapter implements
the storage port. CLI, stdio MCP, authenticated HTTP MCP and the read-only dashboard share it. The optional
Pi installer provides an always-on systemd service; collection/refinement stays
on the Mac. Optional Mac MCP tools let the coding agent recall, inspect PR evidence
and automatically remember supported lessons; `collect` retains its dry-run mode.

Proposed flow:

```text
Mac agent: recall → read PR evidence → extract lessons → MCP write
                                                          ↓
Pi:                                              SQLite knowledge library
                                                          ↓
Agents on Mac or elsewhere ← MCP read tools ← Pi HTTP service (or stdio/SSH)
```

## Implemented storage foundation

- `schema_migrations` records version, name and UTC application time. Migrations
  run in one immediate transaction and preserve existing data. Unknown/newer or
  non-contiguous histories are rejected. Read-only access requires current schema.
  Keep applied migration files immutable; add a new numbered migration for changes.
- `pull_requests` identifies a source revision by repository, PR number and full
  head SHA. This stores source metadata, not complete PR snapshots or diffs.
- `lessons` records candidate content, applicability, asserted producer/method,
  status and a database-assigned storage time. Identity is `(repository, id)`.
- `lesson_evidence` links ordered evidence to a lesson and source revision with
  composite foreign keys. It preserves source URL, optional evidence commit SHA
  and a description. All inserts happen in one transaction.
- Imports accept only complete candidates. Exact retries do not duplicate or change
  the original timestamp; changed content at the same identity is a conflict.
  Conflicting metadata for the same PR revision is also rejected.
- Repository-scoped get/search return evidence and provenance. Search excludes
  deprecated lessons, uses literal substring matching and caps results at 100.
  Explicit get can retrieve deprecated lessons for inspection.
- SQLite uses WAL and a five-second busy timeout. Separate local read connections
  can see committed writes. The database stays local; MCP transports wrap the store.

Validation checks completeness, repository syntax and 40-character commit SHAs.
It does not fetch/verify sources, prove a lesson's truth, authenticate producers
or approve a lesson. Evidence-backed lifecycle events can retire/supersede lessons or flag conflicts;
there is no content-overwrite or delete API. A candidate
must not be treated as an instruction or as a verified rule.

## Agent memory loop

The optional `mac-client` features add on-demand Mac MCP tools for recall, PR
capture, evidence inspection and automatic publication. Codex supplies the reasoning
in its existing session. Scoped personal guidance makes recall part of normal
planning/implementation/review, then learns from PRs read for that task. There is
no per-lesson human approval gate. See [agent memory](agent-memory.md).

The Pi remains the store and read/write MCP boundary. No GitHub/model credentials,
watcher or extraction job run there. Exact normalized content is deduplicated;
agents can now consolidate duplicates by superseding them, retire outdated advice
and flag contradictions with immutable lifecycle evidence. Unattended semantic
scanning remains future work; see [lesson lifecycle](lifecycle.md).
Lessons are usable candidates with evidence and asserted provenance, not verified
rules. Any future verification/edit workflow should be optional oversight.

## Repository scopes

The default `repository` plus an optional complete `repositories` allowlist are
shared configuration conventions for Pi and Mac. All scoped tools select one
canonical repository; the dashboard exposes selection and MCP exposes discovery.
No database migration is needed. See [configuration and rollout](repositories.md).

## Data and access

- Treat source text as untrusted evidence. It cannot grant tool permissions or
  issue instructions to the consolidator or consuming agent.
- Keep GitHub credentials on the Mac with read-only collection access. Keep all
  credentials outside Git. HTTP uses separate read/write bearer tokens; stdio/SSH
  relies on SSH/OS access. HTTP validates Host and rejects browser Origin headers.
- Keep company and personal collections separated. Decide what can be stored on
  the Pi and sent to a model before connecting private repositories.
- Keep the library outside the checkout so code updates do not overwrite it.
  Plan SQLite-aware backups, retention and deletion of derived lessons.
- The Pi template follows SymNotes private LAN/Tailscale HTTP access. Add TLS for
  untrusted networks. Tokens are generated on the Pi and never committed or logged.

Only synthetic PR evidence and lessons are included in this repository.
