# Learning from selected Codex conversations

Dreamer can now store and recall lessons with native chat citations, alongside PR
lessons. Codex selects and interprets evidence in its existing session; the Mac
filters/caches it and publishes through the Pi's writer MCP connection. No extra
model, polling job or full-history crawler runs on either machine. Other AI chat
providers are not implemented.

## Rollout

Update the Mac client before publishing chat records. From the Mac checkout:

```sh
cargo build --locked --release --features mac-client
python3 scripts/install_mac.py --enable-conversations
```

This preserves the allowlist, endpoint, credential path and unrelated Codex
settings, and sets `learn_conversations = true` in `~/.config/dreamer/mac.toml`.
Omitting the flag preserves the setting; existing installations default to false.
Set it false to disable new selected-chat operations. It does not erase retained
local excerpts or knowledge already stored on the Pi. No new token is required.

Commit/push the source changes normally, then pull and run `make deploy` on the Pi.
The installer backs up before additive schema 7. Existing PR data and lifecycle
history are preserved; old binaries cannot open the upgraded database. Rollback
requires the matching old binary and a checked pre-upgrade snapshot, keeping the
new database separately. Update other Mac clients before they read chat records:
old clients with strict JSON decoding may reject the new evidence field.

Start a new Codex task to load updated tools/guidance. Chronology-safe chat writes require schema 7 and refuse an older
Pi before mutation. Existing PR recall still works against it. Deployment is not
complete until the Pi has actually been updated; a local release build is not a
Pi runtime test.

## Agent workflow

Ask, for example: “Learn reusable Gecko-API knowledge from this selected Codex
chat, and distinguish decisions from pending validation.” The maintained
[dreamer-memory skill](../skills/dreamer-memory/SKILL.md) defines the workflow.
Codex's `read_thread` supplies pages; Dreamer does not secretly read chat files.

1. Choose the actual configured repository explicitly. Retrieve a small selected
   page, remove reasoning/commentary/arguments/commands and minimize sensitive
   message content in memory. Preserve source identity and completed-turn status.
2. `prepare_chat_learning` captures that decoded page and returns revision/source
   IDs. Omit `page` to inspect the last successfully completed-page cursor hint.
   `already_processed` avoids re-extraction of identical retained evidence.
3. `read_chat_evidence` delivers one to eight sanitized excerpts and records which
   sources were inspected. Check current docs/code and existing knowledge.
4. Add a narrow stable `knowledge_key`. `assess_chat_lessons` returns related
   same-key knowledge (plus legacy keyless advice from the same chat), source
   chronology and an assessment token. Read the records; give each peer an agent
   assessment as consistent, historical, conflict or correction. This is automatic
   agent work, with no per-lesson user approval.
5. `remember_chat_lessons` publishes up to ten supported drafts, or records a
   specific no-new-knowledge skip reason. Decisions/corrections require supporting
   user statements. Observed outcomes require an inspected actual result and a
   second source. Hypotheses require `Hypothesis (unverified): ` in their summary.
   These are structural checks; the agent remains responsible for interpretation.
6. Exact Pi readback precedes completion. Retry identical drafts after an uncertain
   write. `reconcile_chat_lessons` uses the same evidence for retire/supersede/
   conflict events and can resume saved pending corrections when changes are
   omitted. Unfinished corrections block page completion.

These are usable candidates without individual approval. An assistant's report,
merge, repeated advice or successful tool status is not proof of correctness.
Separate configuration readback from customer acceptance or end-to-end tests.
Past permissions in a chat do not authorize executing the procedure again.

## Evidence and retention

The Pi stores lesson text and typed citations: provider, thread/turn/item identity,
`codex://threads/...` link, observed time, digest, source kind and claim class.
There is no fake PR and no transcript or raw result upload. Existing PR-only JSON
omits the new empty field. Mixed sources round-trip through storage and history;
compact recall retains its 12 KB cap, with full detail fetched only as needed.

On the Mac, excerpts are capped at 1,600 bytes, 60 useful items and 20 completed
turns per page; input is capped at 512 KB. Only user messages, final assistant
reports and explicitly interpreted complete results are retained. Automatic
redaction removes obvious credentials/emails/URLs and long identifiers; it cannot
recognize every name or business value. The caller must minimize text and supply
additional `sensitive_terms` where needed. Never save unfiltered thread JSON.

The content digest identifies the sanitized full message view before excerpt
shortening. Result evidence additionally hashes its inspected output, without
retaining that output. It identifies the observed view, not independently verified
truth or a complete original transcript. Source API truncation can limit that view;
do not infer completeness from the digest. Changed retained items reopen learning
and invalidate their older known revision. Dreamer cannot discover unseen edits;
the agent must revisit the source. A cursor is a progress hint, never proof that
all older turns were reviewed.

Private cache records live under `cache_dir/chats`. Capture prunes completed
revisions older than 30 days and makes room within 128 revisions; pending work is
preserved and a full cache refuses new capture. At most 2,048 item identities are
indexed per retained thread. Once all its revisions expire, its cursor/index may
expire too. Stable lesson IDs and remote readback prevent identical republishing
from duplicating knowledge. Sensitive-term lists are not stored. Activity reports
contain operational metadata, not excerpts, raw arguments or error bodies.

## Local preview

The CLI supports the same bridge for scripts and troubleshooting. `prepare-chat`
reads a JSON object with `repository`, `thread_id`, decoded `page`, and optional
`observations` / `sensitive_terms`; `read-chat` requires selected IDs. Use only
already-minimized input files in a private directory. Draft files are JSON arrays
of `summary`, `applies_when`, `claim`, optional `scope`, and `evidence_ids`.

```sh
DREAMER="$HOME/.local/share/dreamer/dreamer-mac"
SETTINGS="$HOME/.config/dreamer/mac.toml"
"$DREAMER" mac --settings "$SETTINGS" prepare-chat --file private-input.json
"$DREAMER" mac --settings "$SETTINGS" read-chat --repository owner/repo --revision REV --id ITEM
"$DREAMER" mac --settings "$SETTINGS" preview-chat --repository owner/repo --revision REV --file private-drafts.json
```

`preview-chat` validates and prints candidates locally without contacting the Pi.
`remember-chat` publishes; `reconcile-chat` corrects prior knowledge. The normal
agent path uses MCP directly. A preview is not stored knowledge.

## Chronology and backfilling (schema 7)

Capture/storage times describe when Dreamer saw something. They are not when the
conversation established it. `source_time` stores the original time and basis:
`message_timestamp`, `turn_timestamp` or `uuid_v7_turn_start`. The collector accepts
RFC3339 item `createdAt` or turn `startedAt` fields when the source actually
supplies them. Otherwise a valid UUIDv7 turn ID supplies a labelled generation-time
estimate ([RFC 9562 §5.7](https://www.rfc-editor.org/rfc/rfc9562.html#section-5.7)).
It is not an exact message or acceptance timestamp. Missing dates stay unknown;
a thread's updated time and the ingestion time are never substituted. Legacy
payloads are unchanged; their valid UUIDv7 turn IDs can supply a read-time estimate.
Source timestamps are asserted metadata, not authenticated proof of truth.

Use a stable narrow key such as `auth.external-idp.account-matching`; different
contexts need different keys. Do not use a broad component, date, chat ID or new
key to evade existing advice. The key is optional for legacy records/exact retries
and required for new chat lessons. Schema 7 adds an indexed key table; it does not
rewrite old lessons, source JSON, IDs, timestamps or lifecycle records. Legacy
keyless records from other chats still need ordinary recall: keys and dates are
not a semantic contradiction detector.

`assess_chat_lessons` (CLI `assess-chat`, Pi `assess_conversation_candidate`)
returns up to twelve related full records, capped at 100 KB, and a token bound to
current related knowledge. Supply that `assessment_token` and `related_assessments`
with each draft: `id`, `relation` (`consistent`, `historical`, `conflict`,
`correction`) and a specific minimized `reason`. Older context must be historical
or an unresolved conflict. An unsupported new draft fails before any write;
publication preflights the complete batch, then the Pi checks the token again in
the write transaction. Concurrent or same-batch same-key changes can invalidate
it: reassess and retry the same lesson content with refreshed assessment metadata.
Receipts preserve partial outcomes; no completion is claimed for an unfinished
batch. Agent assessment reasons stay in the private Mac receipt, not a truth label
or new permission on the Pi.

Stored/full/compact views expose source range, date precision, older/newer/
overlapping/approximate/undated context and bounded related IDs. Competing advice stays
provisional and visible until explicitly reconciled; timestamps alone do not
silently choose a winner. Ranked retrieval uses source chronology to break equal
relevance scores, not insertion time. Candidate selection uses deterministic IDs;
its existing bounded-pool/truncation warning still applies. Normal compact recall
keeps its 12 KB cap. Dashboard details label source time separately from capture
and storage times. An older item can remain relevant to an older task context;
read applicability and newer related advice before applying it.

The Pi rejects chat-backed retire/supersede unless the evidence is an explicit
correction or inspected outcome with known message dates strictly later than all
affected source and recorded conflict evidence. The replacement itself is not
being displaced and may cite the same correcting source; other active related
advice is still checked. Unknown/mixed-source dates and
turn-start estimates cannot establish exact precedence; record a conflict or use
independently inspected current PR evidence. `assess_lesson_change` preflights
this rule; `change_lesson` rechecks it atomically. Existing PR-only correction
behavior is unchanged; PR source chronology is not newly inferred here. Exact
already-applied retries remain no-ops, even after newer related advice arrives.
A legacy pending unsafe change can now be refused; never fabricate dates or mark
that batch completed. Complete safe pending work before rollout where possible.

Update the Mac client/guidance, deploy schema 7 on the Pi and start a new Codex
task. Updated chat publication refuses schema 6 before knowledge mutations.
Read-only recall still works against schema 6. Update other strict JSON clients
before publishing the additional metadata; rollback needs a matching old binary
and a checked pre-upgrade snapshot, retaining the new database separately.
