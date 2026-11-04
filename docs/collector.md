# Manual Mac collector

The first collector is a read-only GitHub CLI integration. It runs on the Mac,
using `gh`'s existing login; no GitHub credential is copied to the Pi. Collection
never invokes a model, publishes to MCP, or changes the knowledge database.

```sh
gh auth status
cargo run --locked -- collect --repository owner/repo --count 10 \
  --holdout 123 --output data/collections/first-run
```

Replace the repository and held-out PR number. `--holdout` excludes that PR and
limits training merges and proposal-source reviews to before its creation time.
Use `--before <RFC3339>` for an explicit cutoff; when both are supplied, the earlier
one wins. Collection does not read the held-out PR's discussions or diff.
The server's `dreamer.toml` is not needed for this Mac command.

## Evidence and selection

GitHub search scans at most 100 results by default (`--scan-limit`, maximum 1000).
The count is 1–50, default 10. Eligible PRs must be merged, contain at least two
commits, and have a root inline review comment of at least 60 characters matching
an actionable-language heuristic. Replies, short observations and generic approval
do not generate drafts. This is a transparent selection heuristic, not a semantic
quality score. Human and bot reviewers are both retained with their attribution.

The new output directory contains:

- `manifest.json`: selected PR/revision IDs, cutoff, skipped reasons and checksums.
  A complete marker is written only after the preview files exist.
- `pr-N/snapshot.json`: full PR metadata, final file patches, inline comments,
  review summaries, conversation comments and each commit's metadata/file patches.
- `pr-N/diff.patch`: the raw GitHub PR diff, with its SHA-256 in the snapshot.
- `proposals.json` and `report.md`: unverified review prompts with source links,
  component applicability, reviewer identity/type and later same-file commit IDs.

The output directory is private (0700), files are 0600, and a nested `.gitignore`
ignores everything. Existing batches are never overwritten. Keep real captures
out of source control. Tests use synthetic fixtures only.

Pagination checks expected item counts. PRs exceeding GitHub's 250-commit or
3000-file pull API limits are skipped. Commit-detail pagination is bounded too.
Missing API text patches for binary/large files are flagged. GitHub diff timeouts,
authentication/rate-limit failures, changing revisions/counts and incomplete search
results fail the batch instead of silently reporting a successful capture.
Partial evidence may remain; a failed collection has no complete manifest.

API contracts were checked against [GitHub pull request documentation](https://docs.github.com/en/rest/pulls/pulls)
and [commit documentation](https://docs.github.com/en/rest/commits/commits).
Capture is not an atomic historical snapshot of GitHub: comments edited after a
cutoff cannot be reconstructed, so those comments are excluded from proposals.
PR descriptions and review-thread resolution history are not historical archives.

## Drafts, lessons and publishing

`proposals.json` contains rule-based **review prompts**, not importable Lesson
records. The first pilot used agent-assisted manual curation to inspect source
comments, follow-up patches and final test changes before writing candidate
Lesson JSON. An automated semantic consolidator is not included yet.

For each curated lesson, record a specific review check, when it applies, the PR
head SHA, original review link and supporting follow-up/test pointers. Use the
existing Lesson schema with `status: candidate`, `method: manual` and a producer
that identifies the actual curator. This does not authenticate the producer or
make the lesson verified. Later commits are chronology, not proof of causation;
a merged PR does not establish that a finding was fixed. Captured test changes
are not evidence that the tests passed.

The collector has no publish command. After reviewing the local preview, a separate
authorized writer can submit each curated Lesson using `submit_candidate` with
the Pi's write token. The read-token agent connection remains read-only. Scheduling,
automatic consolidation and automated publication are deliberately pending.

## Held-out review protocol

Record the held-out revision and freeze a baseline review before examining the
training discussions. Exclude that PR, respect the cutoff, then compare findings
and additional useful checks after retrieving relevant lessons. Record irrelevant
retrievals and unverified concerns too. One sequential reviewer and one PR cannot
establish a general improvement in bug detection; report zero uplift when observed.

To exercise curated preview lessons through the actual MCP tools locally:

```sh
cargo build --locked
cargo run --locked --example preview_review -- \
  target/debug/dreamer data/collections/curated-lessons.json \
  data/collections/mcp-results.json export event
```

The input is an array of valid candidate Lessons for one repository. The example
loads them into temporary SQLite, launches Dreamer's real stdio MCP server, calls
`search_lessons` and `get_lesson`, and saves a private retrieval log. The temporary
store is removed afterwards; the Pi is unchanged. Output must be a new file.

## Validation

```sh
cargo test --locked --test collector
```

Offline API fixtures cover pagination/truncation, revision changes, cutoff edits,
existing-output preservation and private dry-run output. The full test suite also
checks the shared store and stdio/HTTP MCP behavior. No private PR data is embedded
in the repository's tests or documentation.
