# Correcting and consolidating lessons

Dreamer can supersede outdated advice, retire disproven advice, and record an
unresolved conflict. These actions preserve original lesson content, provenance,
PR evidence and storage timestamps. They append evidence-backed history; they do
not independently verify a lesson, authenticate its asserted producer or guarantee
that the agent's interpretation is correct. No per-lesson approval queue is added.

## Agent workflow

Read the repository's instructions and required docs first. For Gecko-API, the
existing docs contract governs over memory; follow its discrepancy-resolution and
knowledge-capture procedures. Do not duplicate project rules in a new agent file.

After reading a merged/current-head-human-approved PR:

1. Recall and fetch the relevant full lessons. Check applicability, evidence,
   active conflicts and existing replacement/history information.
2. Prepare the PR and read the supporting review/discussion and final code/test
   evidence. The same source guards as candidate learning apply; application tests
   are captured as source, not executed by Dreamer.
3. When new advice is supported, publish its candidate first. Then call Mac
   `reconcile_pr_lessons` with `revision` and up to ten changes:

   ```json
   {
     "revision": "CAPTURED_REVISION",
     "changes": [{
       "source_id": "EARLIER_LESSON_ID",
       "action": "supersede",
       "target_id": "ACTIVE_REPLACEMENT_ID",
       "reason": "The inspected review and final code replace the earlier advice for this case.",
       "evidence_ids": ["review:123", "file:CAPTURED_ID"]
     }]
   }
   ```

   `retire` omits `target_id`. `conflict` references another active lesson while
   leaving both active. Use supersede to consolidate clear duplicates into the
   retained lesson, preserving the redundant lesson's evidence/history.
4. Dreamer revalidates current PR sources, saves pending drafts privately, checks
   the Pi's capabilities and complete batch, then applies and reads back each
   event. The returned acknowledgement is compact; private drafts stay local.

A lifecycle event is atomic on the Pi, but a multi-change Mac batch and replacement
publication are separate operations. If a request fails/disconnects, some earlier
changes may already exist. The private `reconciliation.json` journal prevents PR
skipping while correction work is unfinished. `prepare_pr_learning` identifies a
pending batch. Resume with `reconcile_pr_lessons` and only its `revision` (omit
`changes`); saved inputs are revalidated and exact event identities reconciled.
Before a write starts, rejected preflight inputs can be corrected. After a write
may have started, resume the saved batch before submitting different changes.
Changed PR sources must be recaptured rather than applied from stale evidence.
A successful reconciliation never overrides an incomplete candidate-publication
receipt; retry those original candidate drafts too.

There is no automatic all-library semantic scan or independent model process.
Agents perform interpretation within the user's task. Do not infer a contradiction
merely because two lessons address different contexts; keep unresolved concerns
explicit instead of inventing a winner.

## Pi MCP and CLI contracts

The server's writer-only `change_lesson` accepts `{ "change": { ... } }`:
`repository`, `source_id`, `action`, optional `target_id`, `reason`, asserted
`producer`, and supporting `evidence` using the existing pinned-PR evidence shape.
It returns `outcome: applied|unchanged` and `record` (event ID, canonical change and
SQLite storage time). Raw writer/CLI calls validate structure and repository scope;
they do not fetch GitHub or prove sources were inspected. The Mac wrapper adds
those source/read/review safeguards.

The source and any target must exist and be active in the same repository.
Supersede/retire changes the source status to `deprecated`; source text and evidence
remain unchanged. Targets must be distinct; chains cannot cycle because retired
sources cannot be used as active targets. Repeating the identical canonical event
returns its original receipt even if its target was retired later. A different
terminal transition on an already deprecated source fails rather than overwriting
history or reviving it. Conflict pairs are symmetric for exact retry identity.

Normal search excludes deprecated lessons. A stored lesson can include a small
`lifecycle` summary: replacement/reason or up to ten active conflict IDs (with a
truncation flag). Compact Mac recall includes conflict counts, requiring full
inspection before choosing advice. Once either side is retired/superseded, its
active conflict warning clears; past conflict evidence stays in history.
`get_lesson` includes the first history page. Read-only `get_lesson_history` accepts
`repository`, `id`, optional exact `change_id`, `limit` (1–50, default 20), and
`offset`; it returns `changes` and `next_offset`. Incoming replacements/conflicts
are included. Pagination is live, not a frozen multi-page snapshot.

Local equivalents (existing, current database only):

```sh
dreamer --config /etc/dreamer/dreamer.toml lesson change --file change.json
dreamer --config /etc/dreamer/dreamer.toml lesson history --id LESSON_ID
# Mac: file is an array of change drafts from the captured PR.
dreamer mac --settings ~/.config/dreamer/mac.toml reconcile --revision REVISION --file changes.json
# Resume a saved pending batch:
dreamer mac --settings ~/.config/dreamer/mac.toml reconcile --revision REVISION
```

## Deployment and recovery

Schema 3 adds `lesson_changes` and indexes; existing rows and evidence are
preserved. The Pi installer stops the service, makes a SQLite-consistent backup,
applies migrations and restarts/checks health. After committing/pushing this update,
pull it on the Pi and run `make deploy`. Mac reconciliation requires the upgraded
Pi tools and reports an upgrade error when unavailable; existing recall/candidate
publication remain compatible with the old Pi API. Update the Mac with
`make install-mac` and start a new Codex task to load its tools/guidance.

Older binaries reject schema 3. Rollback needs the matching pre-upgrade binary and
consistent database backup; keep the newer DB separately and stop writers before
any restore. Do not restore over an active WAL database. Scheduled backup/restore
automation and richer dashboard history navigation remain later master-plan work.
