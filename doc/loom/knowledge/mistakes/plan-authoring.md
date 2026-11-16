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

## The Leaf Rule Forced Definition-Shaped Wiring Checks on Stages Whose Consumer Came Later

**What happened:** Over 31 days to 2026-11-07, six disputes came from `wiring` patterns such as `fn run_gate` that match only a definition. A stage that adds a unit whose consumer arrives in a later stage, or outside the plan, has nothing to wire. The cross-stage wiring-coverage warning (and the leaf rule in `version: 1`) still asked for a check, so under `--strict` the author invented one that matched the declaration. It passed while the unit had no caller, and completion's definition-site rule then rejected it.

**Why:** the only way to satisfy the coverage rule was to write a `wiring` entry, and the cheapest entry to write is the definition line. `reachable` was the right proof but doctrine did not prefer it.

**Prevention:** declare `consumer_deferred: "<reason>"` on a stage whose units have no consumer in the plan; prove reach with `reachable`; `loom plan verify` rejects a definition-shaped pattern (`v2_lints/wiring_definition.rs`). Never invent a check to satisfy a coverage warning: a check that cannot fail is a defect (see [tests-that-cannot-fail](tests-that-cannot-fail.md)).

**Fix:** `consumer_deferred`, the lint, and doctrine in `skills/loom-plan-writer`; see [conventions/config-plan-and-gate-rules](../conventions/config-plan-and-gate-rules.md#prove-reach-with-reachable-wiring-is-for-seams-the-graph-cannot-see). The lint skips `^` but not `\b`, `(?m)` or `(pub\s+)?`, so those shapes are still caught only at completion.

## A Plan Ordered a Ritual Integrity Dispute Whose Verdict Cost a Fresh Stage Session

**What happened:** about 49 integrity disputes in 31 days were filed for test edits the plan itself ordered: the stage description said "file one dispute-integrity" for a known event. Each filing retired the stage session, ran a judge, accepted, and re-queued the stage for a fresh session that did nothing but run the gates again. A further ~22 came from detector noise (reflows, multi-line asserts, moved or split tests, tightened ratchets).

**Why:** the detector compared lines and any change to a ratchet file, so ordinary edits raised events, and an accepted verdict always re-queued. A ritual dispute was the only route to a green stage.

**Prevention:** a plan does not order a dispute. It names the expected event and its reason in the stage description; the final reviewer sees the event in `loom hook reviewer-brief` and judges it. The detector compares statements and cancels moves; the daemon finishes a stage whose accepted integrity or impact-tests dispute left a passing committed tree ([adjudication-lifecycle](../architecture/adjudication-lifecycle.md#retirement-and-fresh-session-boundary)). A new recurring cause of filings belongs in a detector fix or a hook proposal in `concerns`, not a plan instruction.

**Fix:** Decisions 8 to 11 of the fewer-disputes plan; [verification-v2-gates](../architecture/verification-v2-gates.md) and [conventions/dispute-and-adjudication](../conventions/dispute-and-adjudication.md#a-dispute-answers-a-wrong-check-never-a-planned-step).

## A Brief Premise Taken From the Plan Did Not Match the Tree

**What happened:** three briefs in the fewer-disputes plan stated a premise the tree contradicted. (a) A brief said the skill's contracts row listed `test` as required; it already read as an optional hint, and another brief asked for a leaf-rule sentence and a README "30 s" statement that were not in the files. (b) The plan's run-order section and every stage description named the plan under its `PLAN-` file name; the worktree held it under the `IN_PROGRESS-PLAN-` name, so commands naming the `PLAN-` path failed with file-not-found. (c) A stage premise said the installed spawn guard predates `loom hook reviewer-brief` and appends nothing; it appended a second brief, so the reviewer's prompt held two.

**Why:** briefs were written from the plan text, not from the tree the worker would edit, and the plan's file is renamed when the run starts.

**Prevention:** a worker verifies each premise it can check cheaply (`rg` the sentence it is told to edit) and reports a mismatch instead of inventing the edit; a plan names the file it will have at run time (`IN_PROGRESS-`); a stage whose behaviour depends on the installed hooks checks the installed copy.

## A Plan Carried Deferred Designs and Shipped-Work History

**What happened:** PLAN-full-suite-at-gate kept a status narrative of work already built, an evidence section, a known-limits list, sibling-plan defects and a "Deferred" section of design options with no owner or trigger. The executable residual work was about 300 of its 524 lines.
**Why:** the plan was treated as documentation of a topic, not as a list of work to do.
**Prevention:** a plan holds only what will be done: the decision it implements, the work items, ordering constraints, acceptance. Built work gets one status line; design options, limits and revisit conditions go to doc/loom/knowledge/concerns; defects of another plan go into that plan.
**Fix:** cut the plan down to status, decision, residual work, ordering and acceptance; deleted the Deferred section.

## A Markdown-Lint Criterion Must Run From the Repository Root

**What happened:** `web/node_modules/.bin/markdownlint-cli2` run with cwd `web/` does not apply the repository-root `.markdownlint.json`, so MD013 line-length errors appeared on `web/src/plan/ui/DESIGN.md`.

**Prevention:** a criterion that lints markdown changes to the repository root first, then calls the binary by its `web/node_modules/.bin/` path.

## Plan Authoring in a Main Checkout With Old `.loom/work` State Has No Dry Run or Source Graph

**What happened:** `loom plan verify --strict --dry-run` in the main checkout failed before running any criterion when `.loom/work` existed without `main-checkout-identity.json` ("dry run: failed to check out the committed HEAD ... checkout identity missing or unreadable"). `loom map --outline` failed the same way.

**Prevention:** the operator archives the old state and runs `loom init` before plan authoring there; until then the plan author has neither a dry run nor the source graph, and says so in the plan instead of citing an unrun dry run.

## A Wire-Type Change Breaks Parity Tests Across the Rust/Web Boundary

**What happened:** the planner bridge's `Item::User`, `SessionView` and `PlanMessage` changes broke the Rust parity test `tests_chat.rs` (it reads web fixtures) and the tag parity test `plan_message_wire_values_are_in_plan_schema` (it reads `web/src/plan/api/schema.ts`).

**Prevention:** a plan gives the web fixture edits to the worker that changes the Rust type, and tells the Rust wire worker which tag-parity failure to expect until the web worker adds the tag. [New PlanMessage Variants Need Both Schema Arms](../conventions/plan-web.md#new-planmessage-variants-need-both-schema-arms) holds the schema half.

## Proposed Claude Code's Advisor Tool Where loom-advisor Already Covers the Need

**What happened:** a model-allocation plan proposed attaching Claude Code's advisor tool (`claude --advisor <model>`) to stage sessions and making an in-session fable consult the top of the escalation ladder, first restricting it to opus and sonnet over a Fable-consent edge case. The user rejected both: loom already has `loom-advisor`, a read-only subagent the orchestrator spawns (fable by default, model overridable at spawn), reserved for the hardest cases after the ladder fails.

**Why:** a new platform feature was treated as a gap in loom's design without checking that the existing mechanism already covered the need; the feature's name matched an existing agent, which compounded the confusion.

**Prevention:** before adopting a platform feature, name the loom mechanism that covers the same need and state what the feature adds that it cannot; when nothing, keep the status quo. The orchestrator follows the escalation ladder (haiku, sonnet, opus medium, opus high) and spawns `loom-advisor` only after that fails.

**Fix:** the plan dropped every `--advisor` key, flag and doctrine line.

## A Pass-Cache Contract Whose Criterion Contains `$` Can Never Be Stored

**What happened:** A plan's contract checked that the acceptance pass cache tracks a directory's contents. Its criterion was `test "$(cat "$LOOM_SHARED_CACHE/v")" = 1`, and its scenario said the first pass "is stored". `is_cacheable` refuses any command containing `$` (`verify/criteria/cache.rs:66`). So the contract was red for a correct implementation, and without the stored check it was green for every implementation.
**Why:** The scenario was written from the cache's intent, without reading the cacheability filter.
**Prevention:** A criterion meant to be cached contains no `$`, `~/`, `mktemp`, URL or `git`. Put the variable read in a committed shell script that the criterion runs with `sh`, as the `InputFixture` of `verify/criteria/tests/cache_input_runner_tests.rs` does. Assert a store (`cache_records() == 1`) and a cached rerun before asserting the miss.
**Fix:** The plan's `pass-cache-tracks-shared-cache-contents` contract runs a committed script and asserts store, hit, then miss.

## Opus Took Visual and Review-Finding Fixes, and Untyped Spawns Inherited Opus (2026-11-14)

**What happened:** `loom usage --since 14d` over plan worktrees showed opus taking 48% of
plan-execution tokens. Sonnet orchestrators made 22 `loom-senior-software-engineer` spawns at
`effort: medium` for visual fixes ("V2 visual fix round 2", "Changes page") and review-finding
fixes ("Fix review findings round 1"). Outside stages, `general-purpose` spawns inherited the
parent's opus: 70M weighted tokens. Every usage row reported `stage_id: null` and requested model
`unknown`, so the table had to be grouped by hand from worktree directory names.

**Why:** the doctrine said opus for "complex debugging and cross-cutting work" with no exclusion
for visual or CSS fixes or for fixes to code a cheaper tier wrote, so any hard-looking fix went up
a tier. An untyped spawn inherits the spawning session's model, and no hook set one outside a
stage. Attribution existed only through hook ledgers, which plain sessions do not write.

**Prevention:** opus takes architecture across modules, new algorithms, concurrency, distributed
systems, security-sensitive code and bugs of unknown cause; a fix for a review finding returns to
the tier that wrote the piece and escalates one tier only on a failed fix; a piece between sonnet
and opus goes to sonnet. Spawn by agent type; the spawn guard rewrites an untyped spawn outside a
stage to `model: sonnet` and denies it inside one
([hook-system](../architecture/hook-system.md#spawn-guard-untyped-spawns-loom-hooksspawn-guardsh)).
When tokens look misallocated, run `loom usage --since <d> --stage <id> --json` and read
`agents.by_agent_model`: a requested model of `unknown` or `default` marks a spawn that inherited
([attribution](../architecture/usage-evidence.md#attribution-without-a-hook-ledger)).

**Fix:** doctrine in `skills/loom-orchestration/SKILL.md` Rule 7, `CLAUDE.md.template` Rule 7 and
the agent files ([Model Playbook](../patterns/subagent-hierarchy.md#model-playbook-orchestration-model-is-per-stage-type-configurable));
the senior agent's frontmatter effort is `medium`.

## Doctrine Surfaces Wrote Codex as the Default Lane (2026-11-14)

**What happened:** the plan said "codex first" and the doctrine workers wrote "codex is the default
lane for every codex-fit piece" on every surface. The user corrected it: the goal is codex WHEN
APPROPRIATE.

**Why:** a worker copies the plan's wording, and the plan wording was a routing order, not a
description of what codex is good at.

**Prevention:** state codex as a lane chosen per piece by its nature and by quota, never as a
default, first or preferred lane, on every surface (templates, skills, agent files, Rust doc and
warning text, signal text). Claude keeps verification, review, planning, exploration, debugging
and anything that iterates on failing tests; codex takes straightforward, well-specified
implementation. After a doctrine change, `rg -n 'preferred|first|default lane'` over the surfaces
in the keyword table; the reviewer found three leftovers of this kind (`preferred()` in the
public API, "preferred lane first" in the plan-writer skill, "the first-listed lane is where most
of the work sits").

**Fix:** `Implementers::preferred` became `first_listed` and every surface says list order means
nothing ([codex-plugin](../architecture/codex-plugin.md)).

## An Unscoped Doctrine Rewrite Orphaned the README and the Cap (2026-11-14)

**What happened:** the doctrine stage rewrote Rule 7 across the skill, template and agent files
while `README.md:327` and `README.md:1187-1199` kept the old opus effort, terra/luna fallback and
"untyped subagent silently inherits" text, and the size caps (`tests_size.rs`) had to be raised
twice (`CLAUDE.md.template` 20,131 to 20,388 bytes, the skill 20,530 to 21,794).

**Why:** the README restates Rule 7 and no test pins it; the byte caps measure the new doctrine
and the plan authorized a raise only to a stated ceiling.

**Prevention:** a plan that changes Rule 7 lists `README.md` among the surfaces and gives
knowledge-distill the rewrite, or the stage changes it. Raise a cap to the measured size, no more.

## Plans remain independent of the invoking harness

**What happened:** The Codex-primary proposal audit initially suggested explicit driver selection in plans or session-start controls. The user requires one plan to run seamlessly from either Codex or Claude Code without choosing a driver.

**Why:** The initial design conflated the harness running Loom with a plan's role/model policy.

**Prevention:** Keep plan semantics harness-neutral. Resolve the invoking harness at runtime; configure model and effort defaults by harness and role/stage type. Plans may override models/effort for roles or stages without requiring a driver field. Persist effective harness identity for recovery and evidence rather than asking the plan author to select it.

**Fix:** Revise doc/PROPOSAL-2026-11-15-codex-as-driver.md around automatic harness binding, per-harness defaults and optional compatible plan overrides; define ambiguous standalone launches and resume behavior explicitly.

**Proposal decision:** An omitted implementers key should mean the active harness lane, preserving the intent documented in loom/src/models/stage/implementers.rs:45. Explicit lists remain exact restrictions. Preserve omission through parsing/init; recover old run provenance from the pinned plan when available and do not reinterpret already authorized workers.

## A Two-Filter Cargo Criterion Passed Plan Verification (2026-11-15)

**What happened:** The test-git-support plan shipped the foundation criterion `cargo test --lib test_git:: verify::transitions::` and seven brief verify lines in the same form; `loom plan verify --strict --dry-run` reported 0 errors and 0 warnings. Cargo rejects the second filter (`error: unexpected argument`), so the criterion could never pass. A pressure test found it.

**Why:** the cargo adapter reads the usage error as a build failure (`src/testrun/adapters/cargo_test.rs` `BUILD_ERROR`), and the dry-run reports any red criterion only as a Note (`src/commands/plan/verify/dry_run_classify.rs`). The rule in [subagent-briefing](subagent-briefing.md) covered briefs, not acceptance lines.

**Prevention:** several libtest filters go after `--` (`cargo test --lib -- a:: b::`; the adapter sums `running N tests`). Read every dry-run Note for a criterion that should already pass, and run a new cargo command line once before writing it into a plan.

**Fix:** the plan and briefs use the `--` form.

## Stating one codex lane mode as the whole lane

**What happened:** While pressure-testing the native Codex worker proposal (2026-11-15), the planner told the user that the legacy codex lane runs `danger-full-access` and relies on the session sandbox alone. The user corrected it: on Linux the lane normally runs through the companion, where Codex applies its own nested bubblewrap `workspace-write` sandbox; only the direct fallback (always on macOS) runs `codex exec --sandbox danger-full-access`.

**Why:** An exploration report described `_codex-direct.py` only, and the planner generalised it without reading `architecture/execution-containment.md` (the lane table) or `loom-hooks/codex-forward.sh` (mode selection).

**Prevention:** Before describing how the codex lane is confined, read the mode table in `architecture/execution-containment.md` and name the mode (companion or direct) with every claim. A design that replaces the lane states its sandbox choice per mode.

**Fix:** The plan keeps Codex's `workspace-write` sandbox when bubblewrap nests inside the worker boundary and falls back to the external-sandbox policy otherwise (PLAN-native-codex-workers, Decision 4).

## A Native-Worker Plan Mounted the Operator's Whole ~/.codex Read-Write and Called It Parity

**What happened:** the native Codex workers plan bound the real `~/.codex` read-write into the worker boundary and said inherited Codex configuration was "at parity with the legacy lane". The legacy capsule denies writes to `.codex/hooks`, `hooks.json`, config.toml and AGENTS.md even inside its `~/.codex` grant, so the plan was weaker. A worker could rewrite rules, hooks, config or the codex package symlink for the operator's next interactive Codex, and `W/.claude` stayed writable. The plan also treated Codex's own sandbox as a second enforcement layer, though execpolicy allow rules, hooks and MCP commands run outside it.

**Why:** the sandbox question was answered from the plan's Decisions alone; nobody compared the new boundary's mounts with the legacy capsule's deny list, and nothing tested the codex-home mounts.

**Prevention:** when a plan replaces a confinement layer, diff its mounts and denies against the layer it replaces before claiming parity. Treat the Loom boundary as the only enforcement and Codex's sandbox as defence in depth. Give every mount of a credential or config directory a test that asserts the host-side effect.

**Fix:** the plan mounts a private tmpfs Codex home holding only `sessions/`, the auth files and AGENTS.md, binds `W/.claude` read-only, and adds the contract `boundary-codex-home-is-private`.
