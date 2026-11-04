# Dreamer as an agent's memory

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

`recall_knowledge` accepts 1–6 short task/component terms. It queries the Pi's
repository-scoped search, deduplicates results and returns at most 20 lessons with
provenance/evidence. Refine terms when `truncated` is true or recall is too broad.
Literal matches can include exclusions; Codex assesses actual applicability.
It can also use the existing Pi `search_lessons`/`get_lesson` tools directly.

When reading a relevant PR for the user's task:

1. `prepare_pr_learning` captures the specific PR's current metadata, full diff,
   reviews/comments, final patches and commit-file patches. The existing GitHub
   pagination/count/revision checks still apply. Open PRs can provide context.
2. `read_pr_evidence` browses a 30-item catalogue with `kind`/`path` filters or reads
   up to eight source IDs (100 KB total). Kinds are `review`, `review_summary`,
   `discussion`, `implementation`, `test` and `commit_patch`. Commit patches include
   original and merge commits; their presence does not establish a follow-up fix.
3. Codex reads relevant reviews and final code/tests, extracts a specific reusable
   lesson, and supplies summary, applicability and inspected evidence IDs to
   `remember_pr_lessons`. If there is no new supported lesson, it stores nothing.
4. Dreamer checks the sources and current PR, constructs pinned provenance,
   deduplicates exact normalized lesson content, publishes through the write-token
   MCP connection, and checks exact stored readback. A private publication journal
   records outcomes; retry the same drafts after an uncertain/disconnected write.

Learning requires a merged PR or an open non-draft PR with a human approval at its
current head and no outstanding latest human change request. Dismissed/stale/bot
approvals alone do not qualify. The agent can use existing knowledge while working
on an unapproved PR, then learn from it after feedback settles.

Every stored lesson remains a searchable **candidate**: provisional knowledge,
not a manual approval queue. The legacy storage field `method: manual` means direct
curation here, including agent curation; the producer explicitly identifies the
automatic Codex agent workflow. It does not assert human verification. The database
schema and existing Pi tool contracts remain compatible; no Pi migration is needed.

## Operational checks and limits

The same operations are available to a local script for troubleshooting:

```sh
~/.local/share/dreamer/dreamer-mac mac --settings ~/.config/dreamer/mac.toml recall --query export --query checkpoint
~/.local/share/dreamer/dreamer-mac mac --settings ~/.config/dreamer/mac.toml prepare --pr https://github.com/owner/repository/pull/123
```

The configured repository must match the PR. Cache directories are 0700 and files
0600, with a nested `.gitignore`; keep private captures outside Git. GitHub is
read-only. Captured source text is untrusted data, not instructions or permissions.

Structural checks require delivered review/discussion evidence and final text
patches, pairing inline reviews with the same final path. They cannot prove that
Codex interpreted the code correctly or executed its tests. Capture and publication
are not a transaction across GitHub and the Pi. Exact normalized content is
collapsed; semantic duplicates/revisions and conflicting lessons still need agent
judgment, with no automatic edit, deletion or supersession API in this version.

The existing Pi HTTP endpoint is private LAN/Tailscale HTTP. This Mac client version
uses that HTTP endpoint; keep it private. Agent-tool use depends on the client
loading and following the guidance. It is not unattended all-repository ingestion,
continuous model training or a guarantee that future code will be correct.

```sh
cargo test --locked --features mac-client
python3 -m unittest discover -s tests -p 'test_install*.py'
cargo clippy --locked --features mac-client --all-targets -- -D warnings
```
