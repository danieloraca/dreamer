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
