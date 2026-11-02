# Plan Authoring

> Plan gaps

## Web stages left the embedded bundle stale

**What happened:** In the planner-ui-redesign plan only design-foundation rebuilt and committed web/dist. kickoff-session, feature-ui and document-ui changed web/src but did not commit a rebuilt dist. The installed binary therefore served the old frontend against the new plan-server API, so the planner looked unchanged and broke in many places.
**Why:** web/dist is committed and embedded at compile time (architecture/web-dashboard.md). The later stages' files globs did not include web/dist, and the plan rebuilt it only for integration-verify review fixes.
**Prevention:** Any plan whose stages change web/src either lists web/dist in each such stage's files and requires a rebuild-and-commit (with an acceptance check that dist matches src), or has a final stage that always rebuilds and commits web/dist.
**Fix:** Run bun run build in web/, commit web/dist, then reinstall the binary with dev-install.sh.

**Recurrence:** Commit 31561332 added the `disposition` issue field to the wire and to `web/src/api/schema.ts` without rebuilding `web/dist`. The schema objects are strict, so the old bundle rejected every frame with stage history and the dashboard header read "connection error". A schema or wire change needs the rebuild in the same commit.

## A Plan Handed Its Distill Stage a Side Document

**What happened:** a plan's knowledge-distill stage was told to update a brainstorming report outside doc/loom/knowledge, and the plan's overview pointed at that report for its reasoning instead of carrying it.

**Why:** the report was the working record of the discussion, so the plan author treated it as a live dependency of the plan.

**Prevention:** a distill stage only distills memories into loom knowledge, plus its usual user docs. A plan is self-contained: decisions and evidence that matter go in the plan text, and it never names a discussion document as an input or a deliverable.

**Fix:** the plan's reasoning moved into the plan, and the distill stage's task and files entry for the report were removed.

## A Pressure Teammate Held Its Report For A Side Check

**What happened:** during a pressure test of a plan, one teammate of six kept its final report back until an optional hook-suite run finished, and that run took far longer than its 3-minute estimate. The lead waited about 29 minutes without checking on it, then told the operator the wait was 8 minutes, a figure taken from the last message it had seen and not from the clock. Separately, long reports sent through an idle notification arrived cut off.

**Why:** the brief set no deadline and did not say that findings come before optional checks. An idle teammate with a background job looks like running work, and the lead treated silence as progress.

**Prevention:** a team brief says to report first and follow up on long checks, and to send a final report as several SendMessage messages, because the `result` field of an idle notification is truncated. The lead checks a quiet teammate's transcript as soon as its own estimate has passed, and states durations from `date`, never from the last message seen.

**Fix:** the lead told the teammate to send its report with that check marked pending.

## Re-validating review findings a prior round already folded in

**What happened:** An /address pass on a second pressure review began re-validating all nine findings, though most repeated the earlier round; the operator had to steer it.
**Why:** The pass read the review as new input and did not diff it against the plan first.
**Prevention:** For a later review round, first compare each finding with the current plan, then validate only the residual each finding adds.
**Fix:** Triage against the plan, keep the amendment text that is new, and report the rest as already addressed.

## A startup step scaled with one file's line length

**What happened:** `loom plan` printed nothing for about 44 s. One untracked 467 KB single-line JSON file under `doc/` took 41.85 s in `chunk_sections`, inside `SearchIndex::build`, which ran before the server announced its URL.

**Why:** `references_in_line` recounted the line's backticks for every candidate sentence end in every span, so cost grew with the square of line length. The announce came after every load step, so any slow step delayed the URL.

**Prevention:** Print the URL as soon as the port is bound, and treat input from the document tree as unbounded when writing per-line scans.

**Fix:** `plan_server/loading.rs` runs the load on its own thread. `sentence_ends` in `fs/knowledge/chunker/references.rs` computes sentence ends once per line.
