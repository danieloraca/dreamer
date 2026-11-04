# Architecture and next steps

One crate keeps the initial project easy to navigate. Modules define the future
boundaries without running workers, opening a database or starting a server.

Proposed flow:

```text
Read-only GitHub source → persisted PR snapshots → candidate consolidation
                                                → lesson library → MCP tools
```

1. Add SQLite migrations for source snapshots, evidence, lessons and sync cursors.
   Use a stable identity based on repository, PR and revision; retries must be
   idempotent. Commit data and cursor updates together. Implement repository-scoped
   lexical search with small result limits before considering embeddings.
2. Implement the GitHub source with merged-PR filtering, pagination, rate-limit
   handling and incremental sync. Preserve diffs, review discussions, follow-up
   commits and final revision references. Begin with 20–50 explicitly selected
   PRs. A review comment alone is not proof that a rule is correct.
3. Implement extraction behind `Consolidator`. Produce candidates with source
   links and applicability. Compare suggestions against final code and tests;
   verification must be an explicit, recorded operation. Track superseding and
   deprecated lessons, contradictory evidence and source deletion.
4. Add an MCP SDK adapter using stdio, with `search_lessons` and `get_lesson`.
   Both tools must require repository scope; get also requires an ID, and search
   requires a query plus a bounded limit. Return status, applicability and evidence
   with results. Test initialization, tool discovery and calls with actual clients.
5. Evaluate a held-out review with and without the library. Add a timer only after
   manual sync is reliable. Keep speculative suggestions out of verified lessons.

`Lesson::validate` currently checks completeness only. The traits describe intended
contracts; they do not enforce store isolation, candidate-only writes or factual
verification. Future adapters must enforce and test these at their boundaries.

## Data and access

- Treat source text as untrusted evidence. It cannot grant tool permissions or
  issue instructions to the consolidator or consuming agent.
- Use a read-only GitHub identity scoped to the chosen repositories. Keep tokens
  outside Git and configuration examples. Authentication is not wired yet.
- Keep company and personal collections separated. Decide what can be stored on
  the Pi and sent to a model before connecting private repositories.
- Keep the library outside the checkout so code updates do not overwrite it.
  Plan SQLite-aware backups, retention and deletion of derived lessons.
- Do not expose an unauthenticated HTTP listener. The first transport should be
  stdio; remote HTTP access needs its own authentication and deployment design.

No real PR snapshots or lessons are included in this repository.
