# MCP setup

Dreamer uses the [official Rust SDK](https://github.com/modelcontextprotocol/rust-sdk)
over stdio. Local tests cover a real SDK client and the legacy `2025-06-18`
initialize/initialized handshake using raw newline-delimited JSON-RPC.

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

`search_lessons` accepts `repository`, `query` and optional `limit` (1–100). It
returns `{ "repository": "...", "lessons": [...] }`, including evidence,
provenance, status and storage time. Matching/order are the same as the CLI.

`get_lesson` accepts `repository` and `id`. It returns
`{ "repository": "...", "found": true, "lesson": { ... } }`, or `found: false`
and `lesson: null`. Deprecated lessons remain available for explicit inspection.

`submit_candidate` is advertised and callable only with `mcp --allow-writes`.
Its argument is `{ "lesson": { ... } }`; see the complete shape in
[the fixture](../fixtures/sample-lesson.json). Responses contain `repository`,
`id`, `status: "candidate"` and `outcome: "inserted"` or `"unchanged"`.
The future Mac collector can use this tool; no collector is included yet.

All tools require the configured repository exactly. Invalid inputs, wrong scope
and conflicts return `isError: true` with a code/message. Unknown/disabled tools
return JSON-RPC method-not-found errors. Producer/evidence remain assertions,
not authenticated identity or verified facts.

At most eight blocking database operations run concurrently per process. SQLite's
five-second busy timeout applies. A write may finish after a client disconnects;
retry the identical candidate to resolve uncertainty without duplicate imports.

## Access and limitations

Read-only mode hides and rejects writes and uses read-only database connections.
The flag controls process capabilities, not individual users. Anyone who can
launch Dreamer with database write permissions can enable submission or use the
CLI. Stronger separation needs distinct SSH/OS principals and file permissions,
or a later authenticated service.

There is no HTTP listener, OAuth, running daemon, automatic collection, model
refinement, verification or edit/delete workflow. Pi runtime and actual client
registration need deployment-host verification; local tests do not establish a
live Pi connection.
