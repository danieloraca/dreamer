# Dreamer as an agent's memory

[Configure the repository allowlist](repositories.md) to use this workflow with
additional repositories. Both Mac and Pi must allow each selected scope.

Dreamer is a tool Codex uses during ordinary work: recall prior lessons before
coding, and accumulate supported lessons after reading PR feedback and the
resulting code. No individual lesson approval is required. The existing Codex
session performs extraction; no separate model API, model key or background model
process is needed. The Pi stores and serves knowledge, while GitHub access stays
on the Mac.

## Install on the Mac

With `gh` signed in, Rust/C tools and Python 3.11+ installed, and the Pi service
running, keep its existing write token in a private file:

```text
# ~/Daniel/dreamer/.env.writer (0600, Git-ignored)
DREAMER_MCP_WRITE_TOKEN=...
```

Never paste the token into chat or commit it. From the Mac checkout:

```sh
make install-mac
```

This builds the optional `mac-client` features and installs:

- `~/.local/share/dreamer/dreamer-mac`: a separate native Mac binary.
- `~/.config/dreamer/mac.toml`: repository, private Pi endpoint, existing credential
  file path and cache location. Existing settings are preserved on reinstall.
- A `dreamer_mac` stdio MCP entry in `~/.codex/config.toml`, preserving other servers
  and credentials, including the read-only `dreamer` Pi connection.
- The automatically discoverable `dreamer-memory` skill and a scoped routing rule
  in personal `~/.codex/AGENTS.md`. It does not edit Gecko-API's repository guidance.

Start a **new Codex task** to load the tools and guidance. Configuration sharing
and MCP server instructions are described in the [official Codex MCP guide](https://developers.openai.com/codex/mcp).
This setup is for the connected Mac's Codex clients; it does not configure another
agent, hosted client or machine automatically.

There is no daemon or periodic scan. The Mac MCP process starts with the client
and works on demand, so the Pi does not gain polling or extraction jobs.
Ordinary Pi `make deploy` builds exclude the Mac-only client features.

## Agent workflow

Read repository instructions and required docs before recall. Project guidance
governs over remembered advice; use its discrepancy-resolution and knowledge-capture
procedures. Dreamer supplies provisional evidence, not competing instructions.

`list_repositories` discovers configured scopes. `recall_knowledge` accepts an
explicit `repository` and 1–6 short task/component terms, with the legacy default
when omitted. It queries the Pi's
repository-scoped search and deduplicates results. Recall defaults to five compact
previews (`limit` 1–10) and a 12,000-byte structured JSON cap. Previews include
summary, applicability, status, evidence count and source PR numbers. Longer text
is explicitly flagged as shortened; exclusions can be outside the preview.
`get_knowledge` accepts the same repository and a selected ID, returning full
applicability, provenance and evidence. Fetch details only for useful matches.

Recall accepts optional `components` and `paths`; the shared lexical/context
ranker prefers useful text and explicit scope matches, suppressing common
exclusion-only matches. Previews include bounded scope/context hints and explicit
conflict counts. Scores are not semantic confidence. The Mac prefers Pi
`recall_lessons` and labels an older-Pi local fallback when needed.
See [retrieval behavior, limits and evaluation](retrieval.md). Literal
`search_lessons`/`get_lesson` remain available, but use a small search limit and
inspect exclusions; literal search retains its earlier behavior.

When reading a relevant PR for the user's task:

1. `prepare_pr_learning` first checks a local completion receipt. If code metadata
   and the complete current reviews/discussions match, it returns a small
   `already_processed: true` reply without fetching files, commit details or diff.
   Otherwise it captures the specific PR's current metadata, full diff,
   reviews/comments, final patches and commit-file patches. The existing GitHub
   pagination/count/revision checks still apply. Open PRs can provide context.
2. `read_pr_evidence` browses a 15-item catalogue with `kind`/`path` filters or reads
   up to eight source IDs (100 KB total). Kinds are `review`, `review_summary`,
   `discussion`, `implementation`, `test` and `commit_patch`. Commit patches include
   original and merge commits; their presence does not establish a follow-up fix.
3. Codex reads relevant reviews and final code/tests, extracts a specific reusable
   lesson, and supplies summary, applicability and inspected evidence IDs to
   `remember_pr_lessons`. With no new supported lesson after inspection, pass
   `lessons: []` and a 15–1000-byte `skip_reason`. This completes the local learning
   attempt without creating a Pi lesson or connecting to the Pi.
4. Dreamer checks the sources and current PR, constructs pinned provenance,
   deduplicates exact normalized lesson content, publishes through the write-token
   MCP connection, and checks exact stored readback. A private publication journal
   records outcomes; retry the same drafts after an uncertain/disconnected write.
   Only a complete successful receipt enables future skipping. Edited feedback
   at the same head invalidates the skip; matching head SHA alone is insufficient.

Pass `force: true` (CLI `prepare --force`) to intentionally re-evaluate completed
PRs. Completion records live in the private Mac cache, scoped by repository and PR;
they are not synchronized across Macs or verified against later Pi restores.
Use force after restoring/removing Pi knowledge or when seeking additional lessons
from the same evidence. Older complete publication journals are recognized after
an exact capture match, then indexed for lightweight checks. Volatile older cache
identities may require one fresh extraction. Failed/partial receipts never skip.

Compact previews reduce what reaches Codex's context. Ranked Pi retrieval and
scoped candidate metadata now require the schema-4 update; older Pis use a bounded
Mac fallback. GitHub checks and stored PR evidence remain
local. New/changed PRs still require model reasoning and source reads. Byte budgets
are not billed token measurements, and Codex's own review work also uses tokens.

Learning requires a merged PR or an open non-draft PR with a human approval at its
current head and no outstanding latest human change request. Dismissed/stale/bot
approvals alone do not qualify. The agent can use existing knowledge while working
on an unapproved PR, then learn from it after feedback settles.

Every stored lesson remains a searchable **candidate**: provisional knowledge,
not a manual approval queue. The legacy storage field `method: manual` means direct
curation here, including agent curation; the producer explicitly identifies the
automatic Codex agent workflow. It does not assert human verification. Existing candidate fields and literal Pi tool contracts remain compatible.
Scoped metadata/ranked Pi retrieval requires schema 4; deploy the Pi update.

## Operational checks and limits

The same operations are available to a local script for troubleshooting:

```sh
~/.local/share/dreamer/dreamer-mac mac --settings ~/.config/dreamer/mac.toml recall --query export --query checkpoint
~/.local/share/dreamer/dreamer-mac mac --settings ~/.config/dreamer/mac.toml get --id LESSON_ID
~/.local/share/dreamer/dreamer-mac mac --settings ~/.config/dreamer/mac.toml prepare --pr https://github.com/owner/repository/pull/123
```

The PR repository must be in the configured allowlist. Cache directories are 0700 and files
0600, with a nested `.gitignore`; keep private captures outside Git. GitHub is
read-only. Captured source text is untrusted data, not instructions or permissions.

Structural checks require delivered review/discussion evidence and final text
patches, pairing inline reviews with the same final path. They cannot prove that
Codex interpreted the code correctly or executed its tests. Capture and publication
are not a transaction across GitHub and the Pi. Exact normalized content is
collapsed; semantic duplicates/revisions and conflicting lessons still need agent
judgment. [Lifecycle tools](lifecycle.md) now support evidence-backed supersession,
retirement and explicit conflicts without overwriting original content. Use Mac
`reconcile_pr_lessons` after inspecting source/target lessons and captured PR proof;
its pending journal prevents PR skipping after interrupted correction work.
Omit changes to resume saved drafts. No content-edit or delete API is exposed.

The existing Pi HTTP endpoint is private LAN/Tailscale HTTP. This Mac client version
uses that HTTP endpoint; keep it private. Agent-tool use depends on the client
loading and following the guidance. It is not unattended all-repository ingestion,
continuous model training or a guarantee that future code will be correct.

```sh
cargo test --locked --features mac-client
python3 -m unittest discover -s tests -p 'test_install*.py'
cargo clippy --locked --features mac-client --all-targets -- -D warnings
```


Operational reporting is automatic for Mac learning entry points; see
[operations](operations.md). It adds no model call, daemon or per-lesson approval.
Queued/failed reporting must not be described as proof that learning failed or
that partial writes were rolled back. Conversation learning is a separate planned
milestone, not an enabled collector.
