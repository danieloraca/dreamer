# MCP setup

Dreamer uses the [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
over stdio and Streamable HTTP. Local tests cover real SDK clients and the legacy `2025-06-18`
initialize/initialized handshake using raw newline-delimited JSON-RPC.

## Always-on HTTP

Use [the Pi service installer](pi.md) and [HTTP client example](../examples/codex.http.toml).
The single `/mcp` endpoint accepts a read token for search/get or a separate write
token for candidate submission and evidence-backed lifecycle changes. `serve` reads `DREAMER_MCP_TOKEN` from its
environment; `serve --allow-writes` also requires `DREAMER_MCP_WRITE_TOKEN`. Missing,
weak or identical tokens cause startup to fail. Native MCP requests must use an
allowed Host and no browser Origin. A separate [read-only dashboard](dashboard.md)
uses browser-facing HTTP routes; it does not send browser requests to MCP. Requests are limited to 1 MiB. `/health`
requires either token and reports database readiness. Neither startup mode creates
or migrates the database. HTTP logs go to stderr/journald.

## Local smoke test

```sh
cargo build --locked --release
./target/release/dreamer --config fixtures/demo.toml db init
./target/release/dreamer --config fixtures/demo.toml lesson add --file fixtures/sample-lesson.json
cargo test --locked --test mcp_stdio
```

Use [examples/codex.local.toml](../examples/codex.local.toml), replacing both paths
with absolute paths. The configuration's relative `data_dir` resolves against
that file, not the client's working directory.

The [official OpenAI MCP documentation](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)
describes `codex mcp add` and `[mcp_servers.<name>]` configuration. The equivalent
local registration command is:

```sh
codex mcp add dreamer -- /absolute/path/to/dreamer/target/release/dreamer --config /absolute/path/to/dreamer/fixtures/demo.toml mcp
```

Registering a server in one client does not establish access in another client or
a hosted ChatGPT environment. Verify discovery and calls separately for each.

## Pi over SSH

After pushing these changes and pulling on the Pi, build the Linux binary there
and initialize its database. Then use [examples/codex.pi.toml](../examples/codex.pi.toml),
replacing the user/host and paths. The client runs `ssh -T` and pipes MCP to the
Pi process. No HTTP port, installed service or Pi GitHub credential is needed.

SSH must work from the environment launching the client. `BatchMode=yes` avoids
password prompts hanging startup; use existing SSH key/agent access and a verified
host key. Shell startup messages must go to stderr, preserving protocol stdout.
Launch a built binary rather than building during the startup timeout.

For real knowledge, use a config outside the checkout with an absolute `data_dir`
and the desired repository scope. Run `db init` with that config before launching
MCP. Keep the synthetic demo separate and SQLite on the Pi's local filesystem.

## Tool contracts

`list_repositories` takes `{}` and returns the default and configured allowlist.
See [repository configuration](repositories.md).

`recall_lessons` accepts `repository`, 1–6 `queries`, optional `components` and
`paths`, and `limit` (1–20). It returns ranked active lessons, bounded relevance
signals, matched count and truncation flags. Prefer it for task guidance; see
[retrieval contracts](retrieval.md). Literal search retains its existing behavior.

`search_lessons` accepts `repository`, `query` and optional `limit` (1–100). It
returns `{ "repository": "...", "lessons": [...] }`, including evidence,
provenance, status and storage time. Matching/order are the same as the CLI.

`get_lesson` accepts `repository` and `id`. It returns
`{ "repository": "...", "found": true, "lesson": { ... } }`, or `found: false`
and `lesson: null`. Deprecated lessons remain available for explicit inspection.
The result also includes the first lifecycle history page. Read-only
`get_lesson_history` and writer-only `change_lesson` expose bounded audit history
and evidence-backed corrections; see [lifecycle contracts](lifecycle.md).

For stdio, `submit_candidate` requires `mcp --allow-writes`; HTTP clients need the
write token and a server started with `serve --allow-writes`.
Its argument is `{ "lesson": { ... } }`; see the complete shape in
[the fixture](../fixtures/sample-lesson.json). Responses contain `repository`,
`id`, `status: "candidate"` and `outcome: "inserted"` or `"unchanged"`.
The batch collector produces a local preview. [Mac agent tools](agent-memory.md)
automatically publish source-backed lessons extracted by the coding agent.

Scoped tools require an allowlisted repository. Case-only aliases resolve to its configured spelling. Invalid inputs, wrong scope
and conflicts return `isError: true` with a code/message. Unknown/disabled tools
return JSON-RPC method-not-found errors. Producer/evidence remain assertions,
not authenticated identity or verified facts.

At most eight blocking database operations run concurrently per process. SQLite's
five-second busy timeout applies. A write may finish after a client disconnects;
retry identical inputs to resolve uncertainty without duplicate imports/events.

## Access and limitations

Read-only mode hides and rejects writes and uses read-only database connections.
For stdio, the flag controls process capabilities, not individual users. Anyone who can
launch Dreamer with database write permissions can enable submission or use the
CLI. Stricter stdio isolation needs distinct SSH/OS principals and file permissions;
HTTP separates reader/writer access using bearer credentials.

HTTP enforces bearer roles on every request, with separate legacy session managers
for read/write clients. Read tokens cannot acquire writer permissions by reusing
a session ID. Credentials are removed before dispatch to protocol handlers.
There is no OAuth, unattended collection, standalone model refinement, verification
or content-edit/delete workflow. Lifecycle transitions preserve original records. On-demand agent learning uses the separate Mac tools. Pi installation and actual client registration still require
deployment-host verification; local tests do not establish a live Pi connection.


Schema 5 adds `get_learning_runs` (read) and `record_learning_run` (writer).
Reports contain bounded operational metadata only, scoped by the same repository
allowlist; terminal outcomes cannot be rewritten. See [operations](operations.md)
for retry, freshness and retention limits. The Mac MCP still exposes seven tools;
reporting runs inside its learning calls and adds no agent round trip.

## Conversation sources (schema 6)

Writer discovery includes `submit_conversation_candidate`, sharing the candidate
validation and role/scope boundary. Lessons and lifecycle changes may carry
`conversation_evidence` alongside `evidence` (PR sources); at least one source is
required. Read tools return typed citations, and the dashboard displays chat links
and claim classes. Excerpts remain on the Mac. Mac selected-chat tools and the
update order are documented in [conversation learning](conversations.md).
