# Plan Authoring

>

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

## A plan rebuilt its bundle after the final review round

**What happened:** the planner-issue-fixes plan told integration-verify to rebuild and commit web/dist after its final loom-code-reviewer round. The pressure test caught it before the run.

**Why:** the review gate records each round's change fingerprint and refuses completion when the tree differs (verify/review/gate.rs, covers_review); only *.md prose is exempt, so a rebuilt bundle makes the final round stale.

**Prevention:** a stage that commits a rebuilt web/dist rebuilds it before its final review round, and any fix after that round means a rebuild and another round. A web stage that only builds dist to shoot screens restores it (git restore, git clean -fdq -- web/dist, then an empty git status --untracked-files=all web/dist) before its final round.

**Fix:** the plan's integration-verify order became gate, build, journeys, screens, issue walk, dist rebuild, review, commits.

## A plan declared files repository-relative under a working_dir

**What happened:** PLAN-planner-issue-fixes gave its stages `working_dir: loom` or `web` and declared `files` and the worker tables repository-relative (`loom/src/...`, `web/src/...`), with a note saying so. Retrieval (`context/rank_source/paths.rs::join_working_dir`) and the plan lints (`v2_lints/mod.rs::stage_touches_dir`) join each `files` entry to `working_dir`, so they read `web/web/src/...` and `loom/loom/src/...`. `loom plan verify` raised no error, and the worker-table lint passed only because table and `files` shared the wrong base. A pressure review caught it.

**Why:** older plans in `doc/plans/DONE-*` used repository-relative `files` under a `working_dir`, and the author copied that form.

**Prevention:** every YAML path in a stage with a `working_dir` is relative to it, `files` and the worker table's Files owned cells included; a file outside the package takes `../`. Only briefs, Brief path cells and prose stay repository-relative.

**Fix:** the plan's `files` and worker tables were rewritten to the working_dir base before any run.

## A plan changed behaviour that unlisted frozen tests pin

**What happened:** the planner-review-fixes plan allowed one planned edit of a frozen test. Its own decisions broke six more: a spine test reading the raw vision body, a concept contract pinning passage hrefs to `/files/`, the header test asserting a link named "Home", a chat contract finding the changed-since trigger by role button, the summaries contract asserting quota meters on `SummaryPanel`, and the fold label "Used 3 tools · edited 2 files". It also added a field to `Attachment` without counting the frozen struct literals. The pressure test caught all of them before the run.

**Why:** the author listed frozen contracts from memory of what each one is about, not by reading the assertions that each changed route, name, label or struct appears in.

**Prevention:** for every behaviour a plan changes (route, accessible name, label text, component output, struct field), run rg for that literal over the frozen test files: web contract files, Rust contract_tests modules and the tests/ contract targets with their support directories. Then either keep the frozen behaviour or list the edit, file and owner in a "Planned edits of frozen tests" table. For a new struct field, also run rg for its literal sites ("TypeName {") and give every site to one worker.

**Fix:** the plan kept the Home tab, the button trigger and the whole `SummaryPanel`, and listed the remaining edits in common.md "Planned edits of frozen tests".

## Frozen contracts specced against preconditions nobody checked

**What happened:** A pressure test of PLAN-fewer-disputes found four frozen contracts that no correct implementation could pass: a PUBLIC SURFACE rule (ZeroTests gives Error) contradicting its Decision and its own contract; a lint contract whose plan texts failed `plan::schema::validate`, so the lint never ran; move fixtures with no `Cargo.toml`, so `collection::rust::chain_problems` found no package; a reviewer-brief fixture whose worktree was not `.worktrees/<id>` on branch `loom/<id>`, which `stage_worktree_and_target` requires.
**Why:** Each scenario was written from the behaviour wanted, not from the preconditions of the function it calls.
**Prevention:** For every contract, read the called function's early returns and name each precondition in the scenario (manifest, layout, validation, preamble). Diff each PUBLIC SURFACE rule against the Decision and the contract scenario that test it. A contract plan text must itself pass `loom plan verify`.
**Fix:** The plan's PUBLIC SURFACE now states each fixture precondition, and the scenarios were rewritten to satisfy them.
