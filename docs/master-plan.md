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

Status: implemented and locally verified; Pi rollout pending. This is the first
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

Status: pending. Use the real trial's irrelevant matches to guide this work.

Add explicit component/file applicability and useful relevance ordering while
preserving repository isolation and bounded replies. Reduce matches caused solely
by exclusions. Prefer a measured local search improvement before adding model
calls or infrastructure. Verify useful retrieval, irrelevant matches, exclusions,
response size and compatibility against a fixed representative query set.

## Milestone 3: make operation visible and recoverable

Status: pending.

Expose recent capture/publication/reconciliation outcomes, errors and last
successful learning in the dashboard without exposing credentials or private
source bodies unnecessarily. Show replacement/conflict/history navigation.
Add scheduled SQLite-aware backups, configurable retention and a documented,
exercised restore. Define freshness and recovery signals from observed runs;
avoid background polling/model work on the Pi solely to animate the dashboard.
Verify authenticated read-only visibility, bounded storage, failed-run reporting,
backup consistency and recovery on a disposable database before Pi rollout.

## Milestone 4: demonstrate useful daily behavior

Status: pending; one initial held-out comparison is already recorded.

Trial a small, declared set of real PR tasks. Freeze baseline/revisions and exclude
held-out sources from learning. Record useful checks and confirmed findings,
irrelevant advice, mistakes or guessing prevented, and retrieval/context cost.
Do not select only favorable examples or claim general bug-detection improvement
from one review. Agents should consult project docs, recall relevant lessons and
save/reconcile supported knowledge without a per-lesson approval step. Report
retrieval and application separately; model-weight training is not taking place.

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
