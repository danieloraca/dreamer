# Configurable repositories

Dreamer uses an explicit repository allowlist. Existing settings with only
`repository` continue to mean one repository. An omitted or empty list uses that
legacy default. Add `repositories` as the complete
list, including the default:

```toml
# Pi: /etc/dreamer/dreamer.toml
[github]
repository = "geckolabs/Gecko-API" # default for older clients and local CLI
repositories = [
  "geckolabs/Gecko-API",
  "geckolabs/Gecko-Micro-Apps",
]
batch_size = 25
```

```toml
# Mac: ~/.config/dreamer/mac.toml (top level)
repository = "geckolabs/Gecko-API"
repositories = [
  "geckolabs/Gecko-API",
  "geckolabs/Gecko-Micro-Apps",
]
# Retain your existing endpoint, writer_token_file and cache_dir.
```

These names are examples, not automatically enabled repositories. Add only the
repos you want available. The same repository must be allowed on both hosts for
capture, learning and retrieval to work together. GitHub access still depends on
the Mac's existing `gh` permissions. All requests retain a single repository scope;
there is no automatic search or lesson sharing across repositories.

The default must appear with identical spelling in an explicit list. Invalid
names, wildcards, duplicates and case aliases are rejected. Valid requests that
vary only in ASCII case resolve to the configured canonical spelling so they do
not create a second database namespace. Preserve canonical names when changing
configuration; changing their stored spelling is not a data migration.

## Rollout and configuration

1. Push/pull the updated code and run `make deploy` on the Pi first. Existing data,
   tokens and settings are preserved. No database migration is needed.
2. Edit only the scope fields in `/etc/dreamer/dreamer.toml` with `sudoedit`, then
   run `sudo systemctl restart dreamer`. Reinstallation preserves this list.
3. On the Mac, run `make install-mac` and add the matching list to
   `~/.config/dreamer/mac.toml`. Existing settings and credentials are preserved; a legacy Mac settings file gains
   an explicit one-repo list without expanding access.
4. Start a new Codex task and reopen the dashboard to load current configuration.

Configuration is loaded at process startup. The existing service's access tokens
apply to every repository in its allowlist; this is namespace separation, not
per-repository user roles. Removing a repo blocks access but does not delete its
lessons or cached evidence. Re-enable the same canonical name to access them again.

To eventually include all Gecko repos, list the names visible to your authenticated
Mac and copy the desired entries into the allowlists. This does not ingest them:

```sh
gh repo list geckolabs --limit 1000 --json nameWithOwner --jq '.[].nameWithOwner'
```

The GitHub CLI result is limited to 1000 entries and your account's access. No
organisation wildcard or automatic organisation crawl is enabled.

## Using repositories

Both MCP servers expose read-only `list_repositories` discovery. The Pi returns
its allowlist; the Mac returns its own. When they differ, update configuration
rather than assuming the broader list applies on both hosts.

- Pi read, history, candidate and lifecycle tools validate the requested
  repository against the service's allowlist. A denied scope is never read/written.
- Mac `recall_knowledge` accepts `repository`, `queries` and optional `components`/`paths`. Omission preserves the
  configured default, but agent guidance selects the actual task scope explicitly.
- Mac `prepare_pr_learning` infers the repo from the supplied PR URL and validates
  it before GitHub access. Cached evidence, lesson IDs, provenance and publication
  always retain that captured scope, including after a default-repo change.
- Dashboard `Repository` selection loads scoped counts, search and details. Switching
  clears old results, filters and pagination and cancels pending requests.
- Local commands keep their default and support an explicit selection:

```sh
dreamer --config dreamer.toml lesson --repository owner/another-repo search --query pagination
dreamer mac --settings ~/.config/dreamer/mac.toml repositories
dreamer mac --settings ~/.config/dreamer/mac.toml recall --repository owner/another-repo --query pagination
```

The dry-run `collect --repository` command stays independent from the Pi config. It
only captures local evidence and never publishes. On-demand learning still requires
reading reviews and resulting code; adding a name to a list does not train a model
or automatically extract lessons.

## Verification

Local two-repository fixtures cover identical lesson IDs, scope discovery, isolated
search/get/publication/dashboard counts, rejected unconfigured scopes, PR URL
inference, cache access after scope removal, default compatibility and installer
preservation. No additional real Gecko repo is enabled by these tests. Live
multi-repository Pi use requires deployment and matching host configuration.
