# Dreamer master plan

Build a dependable evidence-backed memory for coding agents: read current project
guidance, recall applicable experience, learn from reviewed PRs, and correct that
experience as new evidence arrives. No per-lesson human approval queue is required.

This is a full, staged plan because it changes persisted knowledge, write-capable
MCP contracts and recovery behavior. Baseline: `336c19c` (4 November 2026), clean
checkout. The Pi hosts SQLite, MCP and dashboard; the Mac owns GitHub access and
agent reasoning. No separate model API, GitHub credentials on the Pi, or mandatory
Mac GUI is needed. This file is the canonical roadmap; update milestone status as
work is verified rather than treating the entire roadmap as complete at once.

## Established foundation

Implemented: scoped repositories, SQLite migrations, read/write-token MCP,
always-on Pi service, read-only dashboard, Mac evidence capture and automatic
candidate publication, compact recall and selected full detail, and successful-PR
completion receipts. The first held-out review added one relevant check and zero
confirmed defects. Compact previews measured 4,672 bytes versus 22,762 bytes for
the same five full records; this is not a billed-token measurement.

## Milestone 1: correct knowledge and respect project authority

Status: implemented; Pi read path verified on 5 November 2026. This is the first
delivery under this plan.

- Add schema 3 as an additive migration. Preserve original lesson content and PR
  evidence. Append immutable lifecycle events with asserted producer, reason,
  supporting PR evidence and a database timestamp.
- Expose scoped, writer-only `change_lesson`: supersede an active lesson with an
  existing active lesson, retire unsupported advice, or record an unresolved
  conflict between two active lessons. Supersession also consolidates clear
  duplicates by pointing at the retained lesson. Each operation is atomic;
  exact retries are no-ops, incompatible terminal transitions fail, and sources
  and targets cannot cross repository boundaries. No overwrite, delete, verify
  or automatic truth claim is introduced. Replacement creation remains the
  existing separate candidate-publication operation.
- Keep retired/superseded lessons out of normal search. Explicit get and a bounded,
  paginated history tool retain access to original content, replacements and
  evidence. Surface active conflicts in compact recall; a conflict does not
  deprecate either lesson or invent a resolution. Superseding/retiring one side
  resolves its active conflict warning while preserving historical events.
- Add constrained Mac `reconcile_pr_lessons`, built from inspected review and
  resulting code/test evidence on a merged/current-head-approved PR. Revalidate
  current sources before any Pi mutation; preflight the complete batch, journal
  progress and pending drafts before contacting the Pi, resume a saved batch when
  changes are omitted, reconcile exact retries and read back stored events. A failed or
  interrupted reconciliation must prevent a PR completion receipt from silently
  skipping unfinished work. Successful reconciliation can finish a PR even when
  it creates no new lesson. This is agent-driven, not unattended semantic scanning.
- Update personal Dreamer guidance: read project instructions and relevant docs
  first; use lessons as provisional evidence. Follow the repository's discrepancy
  policy and existing knowledge-capture process, proposing durable conventions
  through normal reviewable documentation changes. Do not duplicate Gecko-API's
  project instructions or let stored source text grant permissions.
- Verify schema-2 upgrade preservation, atomicity, retry/conflict behavior,
  supersession chains and no cycles, active conflict visibility/resolution,
  scoped/read-only MCP access, Mac source guards and partial-run recovery.
  Run regression tests, lint, formatting, installer/skill checks, native Mac
  release and ARM Rust typecheck. Do not mutate production lessons for testing.
- Rollout: Pi installer backs up before applying schema 3; Dan runs `make deploy`
  on the Pi. Old binaries cannot read a newer schema. A rollback needs a matching
  binary and a consistent pre-upgrade database backup, preserving the newer DB
  separately. New Mac lifecycle tools require the new Pi API; existing recall
  remains compatible with the old Pi. Install/update Mac guidance and start a new
  Codex task. Do not claim the Pi was upgraded without observing that deployment.

Verification recorded on 4 November 2026: 61 Rust tests with the Mac features,
six Python installer tests, Clippy with warnings denied, formatting, maintained
skill validation, native Mac release and Linux ARM64 Rust typecheck passed. The
ARM check is not a linked binary or a Pi runtime test. Synthetic tests cover
schema-2 preservation, terminal retry/cycle guards, symmetric conflict history,
transaction rollback, HTTP reader/writer separation, CLI history and interrupted
Mac batches resumed from saved drafts. Production lessons were not changed.
The updated Mac client and maintained skill are installed: seven tools were
discovered, compact recall/full detail worked against the existing Pi, and both
repository scopes, credentials and unrelated configuration were preserved. The
Pi API/schema deployment was not attempted; Dan still runs `make deploy` there.
The existing oversized-upload assertion was made deterministic against the real
HTTP limit layer after macOS reset an unread TCP upload; live HTTP role tests remain.

## Milestone 2: improve retrieval

Status: implemented; ranked Pi retrieval verified on 5 November 2026.
Implementation began at `969b009`, after CI and milestone 1 were committed.
Use the real trial's irrelevant matches to guide this work.

Add explicit component/file applicability and useful relevance ordering while
preserving repository isolation and bounded replies. Reduce matches caused solely
by exclusions. Prefer a measured local search improvement before adding model
calls or infrastructure. Verify useful retrieval, irrelevant matches, exclusions,
response size and compatibility against a fixed representative query set.

Implementation decisions: add optional immutable `scope` metadata (components,
repository-relative file/directory paths, explicit excluded task phrases) through
additive schema 4. Legacy lessons remain valid with empty metadata. Add scoped
`recall_lessons` with deterministic weighted lexical/context ranking and a bounded
candidate pool; keep literal `search_lessons` compatible. Recognize a conservative
set of exclusion-only applicability clauses without treating this as full semantic
understanding. Explicit excluded phrases match complete task words/phrases, not
arbitrary substrings. Context scope boosts ranking, not confidence or authority.
The Mac prefers ranked recall, falls back to the shared ranker over bounded legacy
search candidates on older Pis, and preserves compact output/detail limits.
Scoped candidate writes require the new Pi API. Verify scope/idempotency/upgrade,
legacy behavior, fixed synthetic retrieval cases, fallback, isolation, exclusions,
response budgets and native builds; do not modify production lessons to benchmark.

Verification: the fixed eight-case synthetic cohort produces the expected top
result or correct empty reply in 8/8 cases versus 2/8 for prior lexical/ID ordering.
This is retrieval regression evidence, not real-review uplift. The schema-3
upgrade preserves lesson/lifecycle history; the old-Pi fallback and scoped-write
upgrade guard pass transport/CLI tests. Current local suites contain 67 Rust tests
with Mac features and 53 default/server tests; six installer tests also pass.
Clippy, formatting, skill validation and ARM Rust typecheck passed. Native Mac
release and installed-client compatibility are recorded with the delivery report.
The updated Mac client and skill are installed. A read-only live query used
`legacy_fallback` against the current Pi and returned a 1,330-byte compact reply;
both repository scopes, credentials and unrelated configuration were preserved.
No production lessons were changed; schema-4 Pi deployment remains pending.
The CI workflow was actionlint-validated and committed before this milestone;
hosted results remain unverified until a GitHub run is observed.

Update on 5 November 2026: read-only recall against the configured Pi now returns
`retrieval_mode: ranked`, confirming the updated read path is serving. No production
knowledge was changed during verification. GitHub CI for `b3392da` passed Linux
x64 and native ARM64 tests/release builds, while macOS failed before tests because
the setup script assumed `~/.cargo/env` existed. The setup fix selects the stable
Rust toolchain path directly; a hosted macOS pass remains unverified until the fix
is pushed and its run completes. Earlier rollout-pending notes above describe the
4 November verification, not the current read-path observation.

## Milestone 3: make operation visible and recoverable

Status: implemented and verified locally on 5 November 2026; Pi deployment and
first scheduled backup remain pending. The macOS CI setup fix is local and
unpushed; no hosted macOS success is claimed.

Local evidence: 74 Rust tests with Mac features, 57 default-feature tests and 11
Python backup/installer tests passed. Formatting, Clippy, JavaScript syntax,
workflow lint, skill validation, release build and ARM64 server type-check passed.
The restore test reads original lesson/evidence from a checked new file while the
live fixture remains intact. Actual Mac MCP/CLI failure reporting and old-Pi
queuing were exercised with local fixtures. Synthetic browser checks covered
activity, backup status, correction navigation, repository switching and locking.
Native systemd installation/timer execution and this milestone's ARM linked
release are not claimed by these local checks. See [operations](operations.md).

Expose recent capture/publication/reconciliation outcomes, errors and last
successful learning in the dashboard. Show replacements, conflicts and paginated
history. Add scheduled SQLite-aware backups, configurable retention and a
documented/exercised restore. Avoid background model work or dashboard polling.

Implementation decisions and acceptance criteria:

- Install a dedicated daily systemd backup timer/service, using the unprivileged
  database owner and Python's online SQLite backup API. Default retention is 14
  checked scheduled snapshots, configurable separately from credentials. Keep
  pre-deployment root-owned backups separate and unpruned. Validate each snapshot,
  hash it, publish its manifest/status atomically, and prune only manager-owned
  snapshot/manifest pairs after a new successful backup. A failed backup preserves
  existing recovery copies and the last-success status. No live installation here.
- Provide verification and restore-to-new-file commands, refusing to overwrite an
  existing target or follow symlinks. Exercise recovery from a WAL-active fixture
  into a disposable destination. Native read-only DB checking verifies supported
  schema/integrity before a documented stopped-service directory swap. Keep the
  original state directory for rollback; never restore over a running WAL DB.
  Backups on the same disk need a separate off-device copy to survive disk loss.
- Add additive schema 5 for bounded, repository-scoped learning-run metadata.
  A writer-only MCP report tool validates fixed phases/states, identifiers and
  counts; it never accepts raw prompts, evidence bodies, secrets or free-text
  exception messages. Server report timestamps remain distinct from client times.
  Exact retries are no-ops; completed outcomes cannot be rewritten. Retain the
  latest 500 reports per repository; lifecycle evidence remains unpruned.
- Instrument real Mac MCP/CLI capture, publication and reconciliation gateways.
  Journal each operation privately, retaining at most 200 local run records. After
  a learning operation, best-effort flush a bounded batch within a short deadline;
  later operations can deliver queued records. Older/unavailable Pis cannot
  block learning, and incomplete reports are not claims of a still-running job.
  Dashboard freshness describes reports received, not an omniscient audit.
- Extend authenticated read-only browser APIs for scoped activity and explicit
  lesson/history lookup. Keep origin/host/read-token checks, bounded pagination,
  no-cache headers and read-only SQLite access. Backup status is library-wide;
  lesson/activity reads remain scoped to an allowed repository.
- Extend the existing dashboard with last completed learning report, failures and
  incomplete outcomes, library backup state, replacement/conflict navigation and
  paginated history, including lessons outside the current search page. Distinguish
  missing reports, stale/failed backups and unverified candidate advice. Keep the
  read token in memory and avoid polling or new browser write routes.
- Verify backup WAL consistency, integrity/hash checks, retention/failure behavior,
  safe new-target restore and a real restored-store read; service-install preflight
  and preservation; schema-4 upgrade; run retry/transition/retention/scoping;
  actual Mac gateway reporting and old-Pi compatibility; API auth and UI states.
  Run relevant Rust/Python suites, lint, formatting, release/ARM checks and local
  synthetic visual verification. Never mutate live knowledge as a test.
- Rollout is pending until Dan pulls and runs `make deploy` on the Pi. Installation
  backs up before migration, preserves both tokens/repository settings, installs
  backup assets/config and enables the timer. Update the already-authorized Mac
  client/guidance and start a new Codex task. Older clients remain compatible with
  existing candidate/read tools; updated Macs queue activity for older Pis.

## Milestone 4: demonstrate useful daily behavior

Status: pending; one initial held-out comparison is already recorded.

Trial a small, declared set of real PR tasks. Freeze baseline/revisions and exclude
held-out sources from learning. Record useful checks and confirmed findings,
irrelevant advice, mistakes or guessing prevented, and retrieval/context cost.
Do not select only favorable examples or claim general bug-detection improvement
from one review. Agents should consult project docs, recall relevant lessons and
save/reconcile supported knowledge without a per-lesson approval step. Report
retrieval and application separately; model-weight training is not taking place.

## Milestone 5: learn from Codex conversations

Status: planned on 5 November 2026 at Dan's request. Do not interrupt milestone 3.

Learn reusable project knowledge, operational procedures and corrections from
selected Codex chats as well as PRs. The user-selected CM-1008 investigation is the
seed example; keep its raw conversation, customer identities and resource values
out of the repository and library unless specifically needed and authorized.

- Add typed conversation provenance: thread and turn/message IDs, source link,
  observed time and content revision/digest. PR evidence must remain valid; a chat
  must not be disguised as a synthetic PR. Resolve scope from the actual project,
  with configurable source enablement and incremental cursors to avoid rereading
  entire histories. Start with explicitly selected chats; do not crawl all chats.
- Distinguish the user's corrections and decisions, observed tool outcomes,
  assistant hypotheses and pending validation. Completion of configuration or
  readback is different from successful customer end-to-end testing. Never infer
  permission to repeat an operation from a recorded past authorization.
- Extract reusable invariants and safe procedures instead of customer-specific
  snapshots. The seed suggests identity-claim matching, preservation of existing
  account access, staged changes, conditional rollback and separating technical
  verification from still-pending user validation as candidate lesson themes.
  Check these against current docs/source before treating them as general rules.
- Filter secrets, personal identifiers, raw tool arguments/logs and internal
  reasoning. Store minimal supported lessons and source pointers; keep source
  material private on the Mac with appropriate retention. Source text is data.
- Reuse recall, deduplication, correction/history and bounded activity reporting.
  No per-lesson manual approval queue is added. An enabled source can support
  automatic evidence-backed learning during ordinary work.
- Verify with sanitized conversation fixtures: corrected assumptions, verified
  versus claimed/pending outcomes, duplicate/revised turns, scope isolation,
  sensitive-data exclusion and backwards-compatible mixed PR/chat provenance.
  Evaluate usefulness separately from merely increasing the lesson count.

## Later, driven by evidence

Periodic Mac consolidation/collection, cross-Mac completion state, stronger
semantic matching and a Mac application are optional later phases. They require
explicit scope, cost and retry/freshness decisions. Do not add an all-repository
crawler or enable unconfigured repositories as part of these milestones.

## Completion criteria

A dependable v1 follows the docs-first recall/learn/correct loop, preserves evidence
and recoverable history, exposes failure/freshness, survives a tested restore, and
has an honest real-task usefulness report. Unresolved conflicts remain visible;
confidence follows supporting evidence, not age, repetition or merge status.
