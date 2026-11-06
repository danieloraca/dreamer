# Context-aware retrieval

Use Mac `recall_knowledge` before code decisions, after reading the project's
required docs. Pass a few task terms and, when useful, the affected components and
repository-relative paths:

```json
{
  "repository": "owner/repository",
  "queries": ["export", "checkpoint"],
  "components": ["exports"],
  "paths": ["src/Export/Clone.php"],
  "limit": 5
}
```

The Mac still returns at most ten compact previews (five by default), with a
12,000-byte structured JSON cap including its wrapper metadata. Full applicability,
exclusions, scope, evidence and history are fetched with `get_knowledge` for useful
IDs. Preview scope/context lists are limited to three items; they are not the full
applicability contract. Scores explain ordering, not confidence or verification.

## Lesson scope

New candidates can optionally include:

```json
"scope": {
  "components": ["exports"],
  "paths": ["src/Export/", "src/Clone.php"],
  "excluded_terms": ["public endpoints"]
}
```

Components are short project tags. Paths are exact, case-sensitive relative files
or directory prefixes ending with `/`; they are not glob patterns. Explicit
excluded task phrases apply to ordered complete words in request terms or
components, not arbitrary substrings (`synchronous` does not exclude
`asynchronous`). The same word normalization used for recall handles punctuation,
CamelCase and simple inflections in exclusions, without partial phrase matching.
Only add scope supported by the inspected PR: a file cited as
evidence does not establish that the lesson applies only to that file. Existing
lessons remain valid with empty scope. Do not rewrite old candidates to add tags;
use an evidence-backed replacement and supersession if a meaningful refinement is
needed. Scoped Mac candidates use a scope-aware stable identity; empty-scope IDs
retain their previous identity convention. Scope remains asserted, provisional
knowledge and is not permission to override project docs.

Limits: eight components (80 bytes each), twelve scope paths (300 bytes each),
eight excluded phrases (200 bytes each). Recall accepts 1–6 terms (200 bytes each),
eight component hints, twenty paths, and a Pi result limit of 1–20. Mac preview
limits remain 1–10. Invalid/traversal/absolute paths are rejected.

## Ranking and compatibility

Pi `recall_lessons` accepts `repository`, `queries`, optional `components`, `paths`
and `limit`. It selects active, scoped candidates inside a consistent SQLite read
snapshot, then uses the shared deterministic ranker. Each query is a word group:

- Punctuation, snake_case and CamelCase delimit words (`BotResponse` becomes
  `bot`, `response`). Common query stop words are removed.
- A small English suffix heuristic handles `replies`/`reply`, ordinary `-s`
  plurals and long enough `-ing` forms such as `streaming`/`stream`. It is not a
  general stemmer, spelling correction, synonym search or semantic model.
- Words can occur across summary, positive applicability and scope fields.
  **Every word in at least one query group must match.** `bot response streaming`
  no longer matches a lesson mentioning only `bot response`. Query groups are
  alternatives, so use a precise behavior/identifier in each; adding a generic
  one-word alternative can admit broad advice. No synonym or semantic inference
  is performed.
- Summary word matches outweigh applicability-only matches. Duplicate/reordered
  groups and overlapping words do not multiply word scores. Word boundaries
  prevent matches such as `provid` inside `provider`.

Scope is a constraint when comparable task context is supplied:

- If both request and lesson have paths, at least one must match. A common
  component does not override disjoint paths. Include known affected consumer
  paths for shared-code changes, not just the shared helper's changed file.
- If both have components, at least one must match unless a path matched.
- Context alone never admits a lesson. A complete task group is still required;
  broad component and path-only fallback have been removed.
- Untagged legacy lessons can still match task groups; missing scope is unknown,
  not evidence of a match or mismatch. Evidence file paths are not inferred scope.

Admitted lessons retain lexical/context scoring; more specific matching paths
rank higher. `matched_queries` lists complete word groups, not exact phrase
occurrences; `matched_tokens` explains normalized words. Scores do not certify
semantic applicability, correctness, novelty or whether the current code already
implements the advice. Ties use source chronology then lesson ID.

Both Pi and Mac replies include `relevance_policy: strict_v1` and
`outcome: matches|no_applicable_match`. A Mac miss returns an empty list and a
short continuation hint, omitting query echoes and lesson metadata. It makes no
`get_lesson` requests, automatic broader retries, model calls or background work.
Continue with current project docs, source and tests; recall again only if task
scope changes or the initial query was mistaken. Conservative gating can miss
useful advice, especially synonyms, shared-code consumers and incomplete scopes.
A miss does not establish absence of knowledge. Truncation flags remain visible.

A conservative clause filter ignores applicability clauses starting with common
exclusions such as `not for`, `does not apply to`, `excluding` and `except`, and a
`, not ...` suffix. Summary safety instructions remain searchable; `not only` is
not an exclusion. This is a documented heuristic, not general language
understanding. Explicit `scope.excluded_terms` provides a more precise opt-out.
Always inspect full text and evidence before applying advice.

Candidate selection uses parameterized literal needles derived from the shared
query tokenizer. SQL requires all needles within a group, with groups as
alternatives; it no longer loads candidates for context alone or partial groups.
Comparable scope is filtered before the cap and evidence loading as well.
This remains broader than the ranker: for example, `repl` selects both `reply` and
`replies`, then word/group checks discard false positives. It never treats user
text as SQL. The first 1,000 matching active candidates in lesson-ID order are
ranked; `candidate_limit_reached` and `truncated` report incomplete/broad retrieval.
`matched_count` counts matches within that bounded pool, not the whole library.
Narrow an overly broad, truncated request rather than fetching more details.
A normal empty reply is a stopping point for recall on the current task. Literal Pi `search_lessons` stays compatible and may
still return an exclusion-only match; prefer ranked recall for task guidance.

The Mac discovers the Pi capability. It applies the same strict gate locally,
including when a previously deployed Pi returns broad ranked hits. Only counts
marked with the same `strict_v1` policy are trusted; otherwise `matched_count`
counts locally admitted candidates. With a Pi lacking ranked recall, it retrieves
up to 100 literal candidates per query and gates them locally. Replies retain
`retrieval_mode: ranked|legacy_fallback`. Older Pis may omit useful candidates
before local filtering, so upgrade both machines for consistent selection and
accurate bounded-pool counts. `search_limited`/`truncated` preserve incomplete
retrieval warnings. Scope-aware submission still requires an upgraded Pi.

## Local usage and rollout

```sh
dreamer --config dreamer.toml lesson recall --query export --component exports --path src/Export/Clone.php
dreamer mac --settings ~/.config/dreamer/mac.toml recall --query export --component exports --path src/Export/Clone.php
```

Schema 4 adds immutable scope rows/indexes without changing existing lesson or
lifecycle evidence. `make deploy` on the Pi backs up before applying all pending
migrations (including schema 3 if still needed). Old binaries cannot read schema
4; rollback requires a consistent matching backup and binary. Install the updated
Mac tools with `make install-mac` and start a new Codex task. There are no new
background jobs, model calls or dependencies for ranking.

The word-group matching update requires both Pi and Mac binaries to be updated:
`make deploy` on the Pi and `make install-mac` on the Mac. It needs no schema or
credential changes. A new Mac can rerank candidates from an older Pi, but cannot
recover word matches that its old whole-phrase candidate selection omitted. The
legacy literal-search fallback has the same discovery limitation and remains
bounded to the existing per-query requests; it does not fan out into extra calls.

## Fixed retrieval evaluation

`fixtures/retrieval-cases.json` contains six synthetic lessons and eight declared
cases. Tests compare the prior lexical/ID ordering with ranked recall, including
exclusion-only export/formatting hits, explicit phrase/word exclusions, word
boundaries, and component/file context without wording overlap. Expected top
result or correct empty result improved from 2/8 to 8/8 on this fixed set. This is
retrieval regression evidence, not a real-review or general bug-detection claim.
The broader real-task trial remains milestone 4 in the master plan.

`fixtures/query-matching-cases.json` adds synthetic phrase/identifier, inflection,
word-boundary and context-constraint regression cases. Each is tested through
SQLite selection and against ranking the complete fixture library, so a ranker
improvement cannot mask missing SQL candidates. Mac loopback MCP tests check
identifier case, compact hints and removal of component-only filler. These tests
establish retrieval behavior, not improved real-review accuracy or token savings.

## Strict-gate verification

`fixtures/strict-relevance-cases.json` adds thirteen synthetic cases modeled on
review-comparison mismatches, plus positive controls: permission-catalog versus
mutation advice, shared UI versus feature-specific advice, consumer-path context,
complete/reordered/inflected groups, scope mismatches, and no context-only padding.
Each case compares SQLite retrieval with the shared ranker over the entire fixture.
Separate tests put 1,001 partial matches or mismatched scopes before a useful
complete match to ensure neither can exhaust SQL's candidate budget. Existing fixture expectations now
reflect deliberate removal of partial/context-only matches; the earlier 2/8 versus
8/8 result describes the previous policy, not new evidence of work-time savings.

Mac tests cover short misses, retained truncation warnings, no automatic detail
fetches/retries, filtering an older ranked Pi and correcting its broad counts,
literal-search fallback, positive previews and the 12 KB cap. These deterministic
checks establish response behavior; repeat controlled task comparisons separately
before claiming token or time savings.
