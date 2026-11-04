# Architecture and next steps

One crate keeps the initial project easy to navigate. The SQLite adapter implements
the storage port. CLI, stdio MCP, authenticated HTTP MCP and the read-only dashboard share it. The optional
Pi installer provides an always-on systemd service; collection/refinement stays
on the Mac. Manual `collect` is implemented there; semantic refinement is pending.

Proposed flow:

```text
Mac: private repo access → collect → local preview → future refinement/MCP write
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
or approve a lesson. There is no status-transition or edit API. A candidate
must not be treated as an instruction or as a verified rule.

## Next steps

1. Install and verify the HTTP service on the Pi. Read/write bearer tokens are
   checked on every request, with separate legacy sessions. The installer creates
   private configuration and a dedicated account, backs up before migrations, and
   enables a hardened unit with restart-on-failure. Shared tools cap database work
   at eight concurrent operations. Stdio/SSH remains available.
2. Calibrate the manual Mac collector's local drafts and evaluation results. It
   captures paginated GitHub evidence with a temporal holdout and completeness
   checks. Incremental sync cursors, retry/backoff and scheduling remain pending.
   The Pi needs no GitHub token. See [collector workflow](collector.md).
3. Implement Mac-side extraction/refinement behind `Consolidator`. Submit candidates
   with evidence; add an explicit, audited verification/supersession workflow
   before enabling edits or promotion to verified lessons. Compare suggestions
   against final code/tests; review comments alone do not establish correctness.
4. Local protocol tests cover SDK discovery/calls, legacy initialization, write
   restrictions, persistence and EOF shutdown. Test SSH and client registration
   separately. Evaluate a held-out review before introducing regular collection,
   semantic search or speculative suggestions.

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
