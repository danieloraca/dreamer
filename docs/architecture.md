# Architecture and next steps

One crate keeps the initial project easy to navigate. The SQLite adapter implements
the storage port; the CLI exercises it locally. No worker or server is running.

Proposed flow:

```text
Mac: private repo access → collection/refinement → future MCP/service write API
                                                          ↓
Pi:                                              SQLite knowledge library
                                                          ↓
Agents on Mac or elsewhere ← future MCP read tools ← Pi service
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
  can see committed writes. This is not a distributed database or a service.

Validation checks completeness, repository syntax and 40-character commit SHAs.
It does not fetch/verify sources, prove a lesson's truth, authenticate producers
or approve a lesson. There is no status-transition or edit API. A candidate
must not be treated as an instruction or as a verified rule.

## Next steps

1. Wrap the storage port in a Pi-hosted MCP/service adapter. Start with read tools
   `search_lessons` and `get_lesson`, then an independently authorized candidate
   submission tool for the Mac collector. Keep writes repository-scoped, validated
   and idempotent. Clients must not manipulate the SQLite file over the network.
2. Build the Mac collector using its existing private-repository access. Preserve
   final revisions and review context; implement pagination, sync cursors and
   retry/rate-limit handling on the collection side. The Pi needs no GitHub token.
3. Implement Mac-side extraction/refinement behind `Consolidator`. Submit candidates
   with evidence; add an explicit, audited verification/supersession workflow
   before enabling edits or promotion to verified lessons. Compare suggestions
   against final code/tests; review comments alone do not establish correctness.
4. Test real MCP initialization, discovery, reads and authorized writes from each
   client. Evaluate a held-out review before introducing regular collection,
   semantic search or speculative suggestions.

## Data and access

- Treat source text as untrusted evidence. It cannot grant tool permissions or
  issue instructions to the consolidator or consuming agent.
- Keep GitHub credentials on the Mac with read-only collection access. Keep all
  credentials outside Git. Service authentication/authorization is not wired yet.
- Keep company and personal collections separated. Decide what can be stored on
  the Pi and sent to a model before connecting private repositories.
- Keep the library outside the checkout so code updates do not overwrite it.
  Plan SQLite-aware backups, retention and deletion of derived lessons.
- Do not expose an unauthenticated HTTP listener. The first transport should be
  stdio; remote HTTP access needs its own authentication and deployment design.

Only synthetic PR evidence and lessons are included in this repository.
