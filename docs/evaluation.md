# Dreamer held-out review trial

On 5 November 2026, two fresh reviewer contexts reviewed three Gecko-API PRs at
frozen commits. Both received the same project guidance and code. One also used
bounded Dreamer recall. **Neither reported a supported defect; this cohort did
not establish an incremental review-quality benefit.** It demonstrated the live
recall and evidence-handling path at low measured retrieval overhead.

## Method

The cohort was declared before reading its diffs or discussions. From a bounded
list of open PRs, select recent non-draft changes with at most 20 changed files and
600 changed lines, excluding the earlier #7869 pilot. Keep distinct areas: #7865
was skipped because its TLS/runtime area overlaps #7870. This gave #7870, #7868
and #7863. It was purposive selection, not a random sample or a known-defect set.

Freeze the actual GitHub base/head commits, their merge bases, all intended diffs,
31 current project guidance files and the 30-lesson library. No selected PR was
cited as a source by the frozen library. Reviewers inspected dependencies at the
pinned heads; the user's modified working `composer.lock` was preserved. Both
reviewed in the same order with the same inherited model/settings. The baseline
was explicitly instructed not to use Dreamer. Neither could use prior reports,
the other arm's outputs, or review discussions. These were separate contexts,
not different models or hardened access-control sandboxes.

Initial reports and input SHA-256 identities were frozen before comparing them
or inspecting withheld discussions. The assisted reviewer used one compact recall
per PR, at most three previews, and at most one selected full record. The used
full lesson exactly matched the frozen library. No evaluation PR was learned
until the comparison was complete. No GitHub comments, code edits or deployments
were performed.

## Outcomes

| PR | Area | Baseline supported defects | Assisted supported defects | Dreamer contribution |
| --- | --- | ---: | ---: | --- |
| [#7870](https://github.com/geckolabs/Gecko-API/pull/7870) | TLS/build runtime | 0 | 0 | Relevant TLS propagation reminder; duplicates a baseline documentation/source check |
| [#7868](https://github.com/geckolabs/Gecko-API/pull/7868) | Calendar invitations | 0 | 0 | No matches for the chosen task phrases |
| [#7863](https://github.com/geckolabs/Gecko-API/pull/7863) | Error-message translation | 0 | 0 | No matches for the chosen task phrases |

No additional confirmed defect or distinct useful check was established. The TLS
reminder is applicable advice, but both reviewers independently inspected that
boundary. The other two TLS previews were not fetched/applied under the bounded
budget; do not label unexamined previews as confirmed useful or irrelevant advice.
A recall miss does not establish that the library has no relevant knowledge, and
no rescue query was used to improve the reported score after seeing the result.

Withheld review evidence raised two historical TLS concerns on an older #7870
revision. The frozen head already preserves the dynamic restore connection's TLS
options and leaves server enforcement for a later release. Those old concerns
are not defects missed at the current head. #7868 has no independent discussion
to use as a completeness oracle. #7863 has a maintainer's stage-success assertion;
we did not repeat that stage test or equate the assertion with independent proof.

## Verification and cost

Both arms ran the pinned isolated TLS/config-cache check and eight Node workflow
checks; those passed. The build guard rejected installed PHP 8.4.23 as intended;
acceptance on the required patched runtime was not exercised. Four calendar PHP
syntax checks and two locale syntax checks passed. Isolated translation-loader
checks resolved the added locale keys; the assisted check also compared base and
head. These are syntax/pure checks, not application test-suite passes. Database,
provider, full framework fallback, live TLS negotiation and deployments were not
executed. Zero review findings do not prove that no defects remain.

| Measured retrieval | Result |
| --- | ---: |
| Compact recall calls / selected full detail calls | 3 / 1 |
| Raw JSON returned, including whitespace | 10,489 bytes |
| Equivalent minified JSON | 8,755 bytes |
| Successful recall/detail elapsed time, summed | 0.478 seconds |

One initial sandbox-denied recall took another 0.026 seconds before the authorized
retry succeeded. Successful-call timing includes the CLI/transport, not model
reasoning. Approximate observed review windows were 289 seconds baseline and 387
seconds assisted, starting after some guidance reading; they are not a controlled
measure of slowdown or savings. API-billed tokens, total orchestration tokens and
monetary cost were unavailable and are not estimated from byte counts.

## Learning and operation afterward

After freezing both arms, the real Mac workflow captured all three PRs. #7870 and
#7863 had qualifying current-head human approval. Their inspected evidence added
no new reusable advice beyond existing lessons/project documentation, so each was
completed with a specific no-new-lessons receipt. #7868 remains context only until
qualifying review/merge evidence exists. No per-lesson approval queue was added.

Readback confirmed five successful real capture/completion activity reports, zero
failed/incomplete reports, and the same 30 lesson records as the frozen library.
No lessons or lifecycle events were added just to increase a count. The Pi's
schema-5 backup report remained healthy. Refresh the dashboard to see the activity.

## Interpretation and follow-up

This small retrospective trial establishes operation and bounded retrieval, not
better defect detection, lower review cost or daily autonomous improvement. Shared
current docs may already contain the same knowledge; source-PR exclusion alone
cannot prove semantic independence. Existing current heads can already include
review fixes, so this is not a sensitivity benchmark with known remaining bugs.

For stronger evidence, record useful applications during future ordinary tasks,
or separately declare a trial of pre-fix revisions with known outcomes and
chronologically restricted knowledge/docs. Do not alter this cohort or mix a later
trial into its results. Conversation learning remains the separate milestone 5.

Private raw code, discussions, immutable reviewer reports, hashes, retrieval
outputs and learning receipts are retained locally under `data/daily-trial-2026-11-05/`.
They are ignored by Git. This report contains the shareable aggregate result.

## Measured cost pilot: 5 November 2026

Six fresh, ephemeral Codex reviews compared three frozen PRs, with and without
bounded Dreamer context. The requested model (`gpt-6.1-sol`), high reasoning
effort, project guidance, base/head commits, output schema and review limits
were identical. Order alternated by pair. All runs completed within six shell
invocations and 240 seconds. Inputs and original reports were hashed before
adjudication. The library was frozen at 58 lessons; no selected PR was a source.

These are observed results, not established Dreamer-caused savings:

| Total across three PRs | Baseline | Dreamer | Change |
| --- | ---: | ---: | ---: |
| Agent review time, including retrieval | 316.55 s | 287.73 s | -9.10% |
| Input plus output tokens | 859,166 | 794,871 | -7.48% |
| Cached input tokens, already included above | 675,712 | 615,552 | -8.90% |
| Uncached input tokens | 173,123 | 168,638 | -2.59% |
| Output tokens | 10,331 | 10,681 | +3.39% |
| Confirmed reported defects | 1 | 1 | Same root cause |

PR #7865 used 8.53% less elapsed time and 18.47% fewer total tokens with Dreamer.
PR #7858 used 0.33% more time and 1.64% more tokens. PR #7839 used 19.49% less
time and 4.41% fewer tokens. The largest time reduction occurred on a task that
rejected every lesson, so ordinary model/tool-strategy variation remains a
credible explanation. One run per arm on three purposively selected tasks does
not establish a general improvement, statistical significance or causation.

Both reviewers found the same newly added CI test contradicting the effective
empty TLS options in #7865. The coordinator reproduced its exact
`Unexpected TLS options for production/tenant` failure using pinned files in a
disposable fixture on PHP 8.4.23 with `pdo_mysql`. No framework, database or cloud
service was accessed. Both CI matrix versions were not executed. Neither arm
reported an unsupported finding; zero migration findings do not prove absence
of bugs or equal review sensitivity. No incremental defect-detection benefit
was established.

Three real compact recalls and one full fetch supplied frozen context to the
assisted prompts: 0.539 seconds and 22,477 returned bytes. Six migration previews
were rejected as inapplicable; one full TLS lesson was used, and two additional
TLS previews were not applied. This isolates the supplied context contribution,
not live MCP schema/discovery/tool-selection overhead. PR descriptions,
tickets and discussions were withheld from the timed task inputs. Current docs
and current knowledge were used, not a chronological backtest. The baseline
reported a missing linked verification reference, and the final assisted review
reported a denied byte comparison; those incidental frictions remain included
and cannot be credited to Dreamer. Coordinator hash checks verified the inputs.

The completed CLI turns supplied actual input, cached-input and output counts;
these were not estimated from bytes. Cached input is a subset, not another
token charge to add. Roughly 80% of baseline input and 78% of assisted input was
cached, so the total-token percentage is not a billing percentage. No monetary
charge was measured. Agent latency excludes human work, cohort preparation,
orchestration, adjudication, collection/consolidation and live MCP overhead.
The six review runs consumed 1,633,025 input tokens (1,291,264 cached) and
21,012 output tokens, plus a separate 17,960-input/five-output capability check;
parent setup/adjudication usage is unavailable. No net workflow saving or
break-even claim follows from these figures.

### Repeating a measurement

The manual Mac runner requires the existing Codex CLI/login and explicit model
and reasoning-effort settings. Put the frozen task, governing guidance, pinned
source identities and any selected knowledge in the prompt. It intentionally
disables automatic user-config MCP startup and memory for controlled inputs;
ordinary Codex tasks retain their existing configuration. Use a fresh output
directory; previous results are never overwritten:

```sh
python3 scripts/measure_review.py \
  --prompt /path/to/frozen-prompt.txt \
  --output data/review-measurements/new-run \
  --schema fixtures/review-result.schema.json
```

The runner saves the final answer and usage/time metadata without retaining raw
reasoning, event transcripts, tool arguments or stderr. Failed/timed-out runs or
missing usage remain incomplete, not zero-cost successes. Six new parser tests
and the existing Python suite passed (18 tests); nine private measurement/math
tests passed. Quality still needs source-based adjudication before comparing
useful cost. No Rust/Pi service change or deployment is involved.

Private protocol, pinned inputs, original reports, metrics, hashes and the
reproduction: `data/review-cost-trial-2026-11-05/`. Post-trial learning is separate:
all three approved PRs were captured only after reports were frozen; two received
specific no-new-lesson receipts, and one source-backed appointment-identity
uniqueness lesson was added and read back. The live library then became 59; it
did not contribute to the frozen trial. Do not reuse that new library while
calling #7839 an unlearned holdout.

The evidence supports measuring ordinary tasks next and reducing irrelevant
context, especially external SSO matches during database-migration work. Repeating
every normal task in two arms would itself add substantial cost.
