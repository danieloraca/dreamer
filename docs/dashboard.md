# Knowledge dashboard

The dashboard is embedded in the same Rust binary and served at `/` on Dreamer's
HTTP port. No Node, frontend build, separate process or new service is needed on
the Pi. It reads the existing SQLite library; it does not ingest, refine, publish,
verify or edit lessons.

After pushing/pulling the change and running `make deploy` on the Pi, open:

```text
http://192.168.0.25:3445/
```

Enter the **read token**, `DREAMER_MCP_TOKEN`, from `/etc/dreamer/dreamer.env` on
the Pi. The write token cannot open the dashboard. Keep tokens out of URLs and
chat. The browser keeps the read token only in the current page's memory, clears
the input after submission, and forgets it on Lock library, reload or close. It
uses no cookies or local/session storage.

## What it shows

- A repository selector for the [configured allowlist](repositories.md).
- Repository totals: all stored lessons, candidate/verified counts, distinct source
  PR numbers and the latest storage date. These are storage facts, not quality scores.
- Paginated lessons (25 per page), literal substring search over IDs, summaries
  and applicability, and all/candidate/verified/deprecated filters.
- A selected lesson's applicability, asserted producer/method, storage date,
  ordered evidence descriptions, HTTPS source links and full commit revisions.
- Learning reports: last received successful learning, failures, incomplete outcomes
  and paginated recent operations. Reports describe the selected repository.
- Last checked backup/failure/staleness for the whole library; see [recovery](operations.md).
- Optional lesson scope, replacement/conflict links and paginated correction history.
  Related lessons can be fetched even outside the current search results.
- Empty, rejected-token and service-unavailable states. Refresh loads current
  knowledge on demand; there is no periodic polling or background work.

Candidates are unverified. The dashboard cannot promote a lesson. Deprecated
lessons are explicitly browseable here for audit; MCP search still excludes them.
Search remains literal matching and may include mentions that are exclusions.
Source text is rendered as plain text, never HTML. Links require HTTPS and open
with no referrer or access to the original page.

## HTTP boundary

`GET /api/knowledge` uses an explicitly selected allowlisted `repository`, defaulting to the legacy
scope when omitted. Responses include the configured names for the selector. It requires the read bearer token, checks the allowed Host,
rejects cross-site browser requests and mismatched Origin, and emits no CORS
headers. It supports `repository`, `q` (up to 500 characters), `status`, `limit` (1–50, default
25) and `offset` (up to 1,000,000). Unknown parameters are rejected.

`GET /api/activity` accepts `repository`, `limit` (1–50, default 10) and `offset`.
`GET /api/lesson` accepts `repository`, `id`, `limit` (1–50, default 20) and history
`offset`; a missing ID returns `stored:null` with an empty history. Both share the
knowledge endpoint's read-token/Host/Origin guards and pagination bounds. Backup
responses contain only selected metadata fields, never raw backup errors.

The HTML/CSS/JS shell is public but contains no credentials or lesson data. A
restrictive Content Security Policy, no-store responses and text-only rendering
protect the view. The API opens SQLite read-only, uses a consistent read snapshot,
and shares the MCP service's eight-operation database limit. It never creates or
migrates a missing database.

The existing `/mcp` and `/health` contracts are unchanged. MCP still rejects all
browser Origin headers. Use this HTTP dashboard on the same private LAN/Tailscale
network as the existing service; use TLS before sending tokens across an untrusted
network. Host/origin checks do not encrypt HTTP.

## Checks

```sh
cargo test --locked --test dashboard
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Integration tests cover scoped counts/pagination, literal filters, empty stores,
read-only connections, authentication, origin/Host checks, method restrictions,
embedded asset headers and a missing database. Browser checks use a separate
local store; they do not change the Pi's knowledge.
