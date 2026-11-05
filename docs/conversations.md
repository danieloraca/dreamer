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
The installer backs up before additive schema 6. Existing PR data and lifecycle
history are preserved; old binaries cannot open the upgraded database. Rollback
requires the matching old binary and a checked pre-upgrade snapshot, keeping the
new database separately. Update other Mac clients before they read chat records:
old clients with strict JSON decoding may reject the new evidence field.

Start a new Codex task to load updated tools/guidance. Chat writes refuse an older
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
4. `remember_chat_lessons` publishes up to ten supported drafts, or records a
   specific no-new-knowledge skip reason. Decisions/corrections require supporting
   user statements. Observed outcomes require an inspected actual result and a
   second source. Hypotheses require `Hypothesis (unverified): ` in their summary.
   These are structural checks; the agent remains responsible for interpretation.
5. Exact Pi readback precedes completion. Retry identical drafts after an uncertain
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
