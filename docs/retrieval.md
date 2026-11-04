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
excluded task phrases apply to complete words/phrases in request terms or
components, not arbitrary substrings (`synchronous` does not exclude
`asynchronous`). Only add scope supported by the inspected PR: a file cited as
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
snapshot, then uses the shared deterministic ranker. Summary matches outweigh
applicability-only matches. Explicit component/path matches increase relevance;
more specific paths beat broad directory hints. Context is a ranking hint, not
proof of applicability. Ties use lesson IDs for repeatability, not age as confidence.

A conservative clause filter ignores applicability clauses starting with common
exclusions such as `not for`, `does not apply to`, `excluding` and `except`, and a
`, not ...` suffix. Summary safety instructions remain searchable; `not only` is
not an exclusion. This is a documented heuristic, not general language
understanding. Explicit `scope.excluded_terms` provides a more precise opt-out.
Always inspect full text and evidence before applying advice.

Candidate selection is parameterized literal matching over text/scope plus
explicit context matches. The most recent 1,000 matching active candidates are
ranked; `candidate_limit_reached` and `truncated` report incomplete/broad retrieval.
`matched_count` counts matches within that bounded pool, not the whole library.
Refine terms/context instead of interpreting an empty or truncated reply as proof
that no knowledge exists. Literal Pi `search_lessons` stays compatible and may
still return an exclusion-only match; prefer ranked recall for task guidance.

The Mac discovers the Pi capability. With ranked recall, context-only matches can
be found even when task wording differs. With an older Pi, it retrieves up to 100
literal candidates per term and applies the same ranker locally. The reply labels
`retrieval_mode: ranked|legacy_fallback`. The fallback cannot discover a record
that old literal search did not return, including context-only matches. Scoped
candidate writes require the upgraded Pi and fail with an upgrade message before
submission. Legacy empty-scope writes remain compatible.

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

## Fixed retrieval evaluation

`fixtures/retrieval-cases.json` contains six synthetic lessons and eight declared
cases. Tests compare the prior lexical/ID ordering with ranked recall, including
exclusion-only export/formatting hits, explicit phrase/word exclusions, word
boundaries, and component/file context without wording overlap. Expected top
result or correct empty result improved from 2/8 to 8/8 on this fixed set. This is
retrieval regression evidence, not a real-review or general bug-detection claim.
The broader real-task trial remains milestone 4 in the master plan.
