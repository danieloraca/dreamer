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

Status: deployed to the Pi on 5 November 2026. Dan's installer output confirms
schema 5 and healthy service startup. The manual backup service returned success
(exit 0), and the live Chrome dashboard showed one checked schema-5 backup. The
daily timer is enabled and waiting; its first scheduled invocation has not yet
been observed. Recovery remains verified on a disposable local store, not by
replacing live Pi knowledge. No hosted macOS CI pass is claimed here.

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
- Rollout procedure (now performed by Dan on the Pi): `make deploy`. Installation
  backs up before migration, preserves both tokens/repository settings, installs
  backup assets/config and enables the timer. Update the already-authorized Mac
  client/guidance and start a new Codex task. Older clients remain compatible with
  existing candidate/read tools; updated Macs queue activity for older Pis.

## Milestone 4: demonstrate useful daily behavior

Status: initial paired trial completed on 5 November 2026; ongoing real-task
usefulness remains to be established. See [the trial report](evaluation.md).

Two fresh isolated same-model contexts reviewed declared unlearned PRs #7870,
#7868 and #7863 at frozen revisions with identical docs/code. Both reported zero
supported defects. Dreamer supplied one relevant TLS reminder already covered by
the baseline's documentation/source check; the other two recalls missed. No
incremental finding, distinct useful check or review-time saving was established.
Three compact recalls and one full fetch returned 10,489 raw JSON bytes and took
0.478 seconds in successful CLI/transport calls; billed model tokens were not
available. This was a small purposive current-head trial, not a randomized or
chronological defect-detection benchmark.

After reports/input hashes were frozen and discussions inspected, real Mac
learning captured the three sources. Two approved heads completed with justified
no-new-lessons receipts; the unreviewed calendar PR remains context only. Pi
readback confirmed five successful activity reports and unchanged lesson contents
(30 records). Do not count duplicate advice or historical already-fixed review
concerns as a gain. The earlier single-PR pilot remains separate evidence.

Trial a small, declared set of real PR tasks. Freeze baseline/revisions and exclude
held-out sources from learning. Record useful checks and confirmed findings,
irrelevant advice, mistakes or guessing prevented, and retrieval/context cost.
Do not select only favorable examples or claim general bug-detection improvement
from one review. Agents should consult project docs, recall relevant lessons and
save/reconcile supported knowledge without a per-lesson approval step. Report
retrieval and application separately; model-weight training is not taking place.


Follow-up on 5 November 2026: the first September and August batches each added
ten scoped lessons, bringing the verified readback count to 50. August inspected
ten of 78 merged PRs, with 68 still outside that batch. A fresh-context repeat of
calendar PR #7868 at the unchanged frozen head retrieved the new August calendar
lesson and prompted RSVP/MIME checks. Both reviewers again found zero supported
defects; the baseline independently inspected the same calendar classes and MIME
contract. No incremental defect-detection benefit was established. Two previews
matching shared task-file context were inapplicable. Successful compact recall
and one detail fetch returned 12,617 raw JSON bytes in 0.115 seconds; this is not
billed tokens or total review time. The repeat changes both the library and the
matcher relative to the earlier trial, so it cannot isolate either effect.
Private collection and repeat-trial artifacts remain under ignored `data/`.

## Milestone 5: learn from Codex conversations

Status: deployed and first real-chat publication/readback verified on 5 November
2026. The Pi advertises the schema-6 chat write capability. The Mac supports
selected Codex pages, typed evidence, local preview, automatic publication and
correction tools. No background/full-history crawler or other chat adapter exists.

Learn reusable project knowledge, operational procedures and corrections from
selected Codex chats as well as PRs. Keep raw conversations, customer identities
and resource values out of the library unless specifically needed and authorized.

Source-access discovery: the existing Codex app `read_thread` tool can access
Dan's selected **Investigate CM-1008** thread
(`codex://threads/01a18759-5a69-79a1-a5f4-4941360a0931`). A bounded three-turn
page was inspected for interface shape, with older turns still paginated. This
was not a complete conversation review or a chat lesson publication. No raw
transcript was saved. The original `Evidence` model required PR provenance. Schema 6 now adds separate
conversation sources through storage, MCP, compact recall, lifecycle history and
dashboard; PR-only payloads retain their prior format.

Implementation sequence:

1. Add backwards-compatible typed conversation evidence alongside existing PR
   evidence, preserving all current records and source links. Keep thread, turn
   and item IDs, source type, observed time and a content digest; do not substitute
   a PR reference for a conversation. Verify additive migration and mixed-source
   reads before adding the collector/publication path.
2. Add an agent bridge for explicitly selected chats and completed task turns.
   Codex provides paginated source material through its existing thread tool;
   the Mac owns normalization and a private incremental journal with bounded
   retention. Resolve repository scope from the configured actual project. Other AI-chat
   sources need an explicit adapter/enablement, not an assumed history API.
   Store cursors as progress hints and content digests as revision identity;
   revised or incomplete evidence must not be marked successfully learned.
3. Filter by item type **before** retaining or normalizing content. The observed
   thread response includes reasoning items, raw tool arguments and command
   text even when outputs are disabled. Exclude internal reasoning and raw
   arguments/commands; retain only needed, sanitized user/assistant messages and
   independently inspected result evidence. Item truncation, summary-only views
   and tool success statuses cannot establish complete business outcomes.
4. Preserve evidence classes for a user correction/decision, an observed result,
   an assistant proposal and an unresolved hypothesis. A user preference can
   support scoped preference memory; an assistant's final claim alone does not
   prove a technical fix. Check project lessons against current source/docs,
   record pending validation explicitly, and keep recorded permissions from
   authorizing future execution.
5. Reuse compact recall, exact readback, source revision deduplication, scoped
   correction/history and activity reporting for the new source type. Keep the
   existing automatic publication model without per-lesson approval. Start by
   proving one bounded selected-chat path with sanitized fixtures, then exercise
   the CM-1008 seed without uploading its full transcript or customer details.

Acceptance: one selected chat can supply a source-cited lesson, a retry adds no
duplicate, a revised turn is re-evaluated, an unverified hypothesis cannot be
presented as an observed fix, and existing PR lessons continue to round-trip.
Tests cover truncation, failed/partial publication, scope isolation, reasoning
and sensitive-content exclusion, mixed provenance and compact response limits.
A real-chat demonstration must distinguish source access, extraction and stored
knowledge; only report publication after Pi readback confirms it.


The seed suggests identity-claim matching, preserving existing account access,
staged changes, conditional rollback, and separating technical verification from
pending user validation as candidate themes. Check them against current source
and documentation before generalizing; customer-specific snapshots do not belong
in the library. Evaluate usefulness separately from the lesson count.

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

Delivery verification: 90 Rust tests with Mac features, 63 default/server tests and
12 Python backup/installer tests passed. Formatting, strict Clippy, maintained
skill validation, dashboard JavaScript syntax and a synthetic DOM check passed.
A native Mac release was built; Linux ARM64 server Rust type-check passed, which
is not a linked ARM binary or a physical Pi runtime test. Tests exercise schema-5
upgrade preservation, mixed sources, source filtering/redaction, revised views,
truncated/status-only result rejection, partial candidate/correction recovery,
old-Pi write refusal, MCP/CLI publication, preview, compact recall and retention.
No production lessons were changed for verification.

The initial local CM-1008 demonstration retrieved two completed turns with more history
still paginated. Four sanitized message sources supplied one explicitly
unverified process hypothesis with native citations in a local preview. No
observed technical/business outcome was established and no chat lesson was
published. Private preview artifacts are in ignored `data/chat-preview/cm-1008/`.
The updated Mac client and managed skill are installed with conversation learning
enabled. A fresh process discovered eleven tools, including the four chat tools.
All three configured repository scopes and the private writer file were preserved.
Read-only live Pi recall still returned ranked results (an empty relevant query,
556 bytes); no production knowledge writes were made during that initial check.
See [conversation workflow and rollout](conversations.md).

Rollout verified on 5 November 2026 after Dan deployed: writer discovery now lists
`submit_conversation_candidate`. A fresh bounded three-turn CM-1008 page supplied
three inspected citations for one provisional external-identity-migration lesson,
`chat-cd71219dd9db8d34af7a8e18`, in the Gecko-API scope. Its summary is explicitly
labelled `Hypothesis (unverified)` and excludes local-auth and AI Access-ID
issuance. Candidate publication completed with exact Pi readback; independent
get, normal ranked recall and the read-only dashboard API all returned it. The
dashboard reports one source chat. Raw transcripts and customer details were not
uploaded. This verifies the first selected-page learning path, not full chat
backfill, a verified technical fix, or improved code quality. Older turns remain
paginated. The private rollout receipt is in ignored
`data/chat-preview/cm-1008-first-publication/`.

## Conversation chronology safeguards

Status: implemented and verified locally; schema-7 Pi rollout remains pending.
Baseline `75cb901`, preserving the prior uncommitted rollout status. Full plan: persistence, MCP compatibility
and destructive correction safety interact, so this is more than a UI change.

Keep source time distinct from capture/storage time, with an explicit basis and
unknown dates preserved. Read source timestamps where supplied; a valid UUIDv7
turn identifier can supply a labelled turn-start estimate (RFC 9562 section 5.7),
not an exact message/acceptance time. Never use capture time or a thread's last
updated time as the original evidence date. Preserve legacy source JSON/IDs.

Add optional repository-scoped narrow knowledge keys in additive schema 7.
New chat publication requires a key; legacy candidates still round-trip and exact
retries remain valid. A scoped assessment tool finds active same-key peers before
publication. Mac learning must surface these peers for agent assessment rather
than silently storing competing advice. Metadata and bounded recall/dashboard
warnings expose older, overlapping, approximate and undated competing material independently
of ingestion order. Keys identify a specific claim and applicability context;
they are not broad components. Semantic equivalence/conflict still needs agent
judgment and current docs/code, not timestamps alone.

The Pi enforces terminal chat correction safety inside the write transaction:
require explicit correction/inspected-outcome evidence, exact message dates newer
than the affected lesson and other related advice, and no ambiguous historical
source. The replacement is not being displaced and may cite that same correcting
source. Turn-start estimates alone never authorize a terminal transition.
Older/equal/unknown evidence can add a conflict/history warning, not silently
retire or supersede current knowledge. Preserve PR behavior and all original
lesson/history data. Check retries before new transition guards; failed batches
retain their existing journal and cannot advance completion. Add new capability
discovery so an updated Mac stops before mutating an older Pi.

Verify old/new ingestion orders against identical fixtures, legacy schema-6
preservation and retries, source-time extraction/unknown dates, topic isolation,
competing/consistent dispositions, older/equal/unknown correction rejection,
fresh correction and unresolved conflict behavior, direct MCP/CLI enforcement,
compact budgets, dashboard labels and Mac interrupted batches. Run both Rust
suites, Python installer/backup tests, formatting, Clippy, skill validation,
native Mac release and ARM64 server type-check. Do not mutate production lessons
for verification. Update Mac guidance/client after tests; Dan deploys the Pi.

Verification: 101 Rust tests with Mac features, 71 default/server tests and 12
Python installer/backup tests passed. Formatting, strict Clippy, skill validation,
dashboard syntax and synthetic DOM chronology/navigation/link checks passed.
Native Mac release and ARM64 server type-check passed (not an ARM linked binary
or a physical Pi runtime test). A real installed v6 client created a disposable
cache; the new client read it and produced identical legacy candidate JSON/ID.
No production knowledge was modified for these checks.

New read-only Pi tools `assess_conversation_candidate` and `assess_lesson_change`
expose comparison/preflight; Mac `assess_chat_lessons`/`assess-chat` supports the
agent's related-lesson assessment. Full batches preflight before candidate writes;
transactional checks reject stale peer snapshots. The target lesson is excluded
from displaced-peer date checks so one fresh correcting source can both create a
replacement and supersede the old record. Exact retries are checked first.
Assessments have at most twelve peers and a 100 KB reply cap; normal compact
recall still has its 12 KB cap and shorter related-ID previews with truncation
warnings. Same-key legacy peers are surfaced without changing their content.

This does not infer semantic contradictions across arbitrary different keys,
authenticate source assertions, automatically declare newer advice correct, or
add PR source dating. Agents must use stable claim/context keys and inspect
current documentation/code. Unknown/approximate chat precedence produces context
or conflicts; safe correction may need independently inspected current PR
sources. No automatic full-history backfill, background model or approval queue
was added. Deploy schema 7 after pushing/pulling the source; keep the installer's
pre-upgrade snapshot for a matched-binary rollback. New chat writes stop before
knowledge mutation against schema 6; read-only recall remains compatible.

Mac installation verified: a fresh process exposed twelve tools, including
`assess_chat_lessons`; installed skill and release binary match the maintained
versions. Three repository scopes, conversation enablement and the private writer
file were preserved. Read-only live ranked recall still returned the original
CM-1008 lesson. A read-only assessment attempt correctly refused the current
schema-6 Pi with a schema-7 upgrade message; no production knowledge was modified.
The schema-7 source changes remain uncommitted/unpushed and undeployed on the Pi.
