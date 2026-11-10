# Summary Store

## Store Layout and Ids

Summaries of the planning documents live in `doc/loom/summaries/` (`store::STORE_DIR`), one Markdown file per id, all
under `loom/src/planning/summary/`. `store::validate_id` accepts `overview/<feature>/product` and `overview/<feature>/engineering` (the overviews,
`store::is_overview`), `<feature>/product/section_<2+ digits>`, `<feature>/product/vision` (a feature's vision, `store::is_vision` and `vision_feature`), `<feature>/metaplan/index` (the feature's metaplan, `store::is_metaplan` and `metaplan_feature`) and `plans/<slug>` (first byte alphanumeric, rest
`[A-Za-z0-9._-]`, no `..`); a feature name follows `kinds::is_feature_name`; the file is `<id>.md`. A feature has one metaplan, `doc/plans/<feature>/index.md`, so `index` is the only accepted stem and `<feature>/metaplan/<other>` is invalid; a second `*metaplan*.md` file is an `Other` document with no summary (`kinds.rs` `plans_kind`: only `index.md` directly in the feature's plan home is `DocKind::Metaplan`; follow-ups are entries under `## Follow-ups` in that file). `summary_id_for` makes an id for a
product section of a feature, for the feature's vision document (`DocKind::Vision`, `doc/product/<feature>/index.md`), for the feature's metaplan (`DocKind::Metaplan`) or for a plan that is the winning copy of its slug and either sits directly in its home
(`plans_dir(feature)`: `doc/plans/<feature>/`, or `doc/plans/` for a standalone plan) or is named by an entry of its
feature's metaplan. `expected_ids` is every such document (the vision summary is listed whenever the vision document exists and sorts first, because `index.md` precedes `section_NN` in path order) plus `overview/<f>/product` (when the feature's vision
exists) and `overview/<f>/engineering` (when it has a metaplan). `Scope` is per feature (`Scope::of(project, overview_id)`):
it lists the overview members in source order (`product`: the vision's rows; `engineering`: the metaplan's entries whose
plan belongs to the feature) and the feature's eligible documents outside both. The vision summary and the metaplan summary are in neither member list nor in `Scope.outside` (the filter drops `is_vision` and `is_metaplan` ids): the product overview keeps citing the vision's sections directly, and the engineering overview cites plans, so overview sealing involves neither. The metaplan summary takes the document limit (`format::MAX_DOCUMENT_SENTENCES`), a plan summary the plan limit (`MAX_PLAN_SENTENCES`, [File Format](#file-format)), a vision summary at most 8 sentences (`format::MAX_VISION_SENTENCES`), all chosen by `format::sentence_limit(id)`, which the rebuild brief also states; a vision summary holds at most 3 paragraphs, a prompt rule only, because no parser counts paragraphs. `validate_id`'s slug rule is stricter than `planning::slug::plan_slug`, so a plan whose slug holds a space silently gets no summary. `summary_path` returns an empty path for an invalid id; every caller
validates first. Adding an expected id changes every summary id list and count in the lib tests and the Rust summary contract targets.

A brief has no summary id of its own. A plan summary covers the plan's brief bundle (`briefs::bundle_files`, the files under `<plan home>/briefs/<slug>/`), so the summary store and the [planning trace](planning-trace.md) share one definition of what a plan is made of. A move between homes (`engine.rs` `with_source`) rewrites only the plan's own `links` and `cites` keys; brief links, brief `cites` keys and `bundle` keys keep the old `briefs/<slug>/` path, so a moved plan with briefs reads stale and is rebuilt ([limit](../concerns/planning-document-model-followups.md#planning-trace-limits)).

The runtime directory (`planning-document-model.md`, Runtime Directory) holds the store's session state outside the
repository: `claims/`, `drafts/<id>.md` and `summaries.lock`.

## File Format

A summary file is front matter, one `#` title, then headings (`#`, `##`) and paragraphs of sentence lines
(`format.rs`, `format/front.rs`, `format/body.rs`). Front matter (`deny_unknown_fields`):

- `kind`: `document` or `overview`; `of`: the source path (document) or the overview id.
- `file`: the source's content hash, document only; a document must not hold `members`, an overview must not hold `file`.
- `cites`: `path#anchor` (or `path` for the preamble) to the hash of the cited span.
- `members`: overview only, member id to `members_hash`, the `normalized_hash` of that member's sentence ids joined by
  `\n` in order.
- `bundle`: plan summaries only, each brief of the plan's bundle (`briefs::bundle_files`) to its content hash, or to `Project::fingerprint` for a brief without Markdown text (JSON mockup, unreadable, oversized). Computed at seal by `evaluate::bundle_hashes`, written only when non-empty; `parse_front` takes the summary id and refuses `bundle` on overviews and on every non-plan document summary (`format/front.rs`). A plan summary of a plan with no briefs that carries `bundle` parses and reads incomplete.

Body rules, each a rejection with a 1-based line: no indented lines, list items, table rows, code fences, block quotes,
headings deeper than level 2, HTML or conflict markers; a heading holds no link; a sentence is one line whose citation
links are a trailing run (`[label](target)` separated only by whitespace) with no link before them. Sentence limits
(`MAX_DOCUMENT_SENTENCES` 15, `MAX_VISION_SENTENCES` 8, `MAX_OVERVIEW_SENTENCES` 30, `MAX_PLAN_SENTENCES` 72;
`sentence_limit(id)` picks one): a section or metaplan summary takes 15, a vision 8, an overview 30 and a plan summary up to `MAX_PLAN_SENTENCES` (72), the parser's ceiling. The seal holds each level to
`PlanCaps::for_source_lines` of the plan's and its briefs' total lines (`store::plan_caps`): detail lines/24 clamped to 15..60, opening a fifth of that clamped to 8..12.

**Two levels in a plan summary.** A plan summary (`plans/<slug>`) holds an opening level above a detail level in one file. A detail
sentence cites the plan; an opening sentence cites its own summary's detail sentences as `[d](#s-xxxxxxxx)`
(`cites_own` and `split_levels`, `format/levels.rs`). Any sentence with a self-link is opening; every other is detail. The seal checks the split ([Seal](#seal)).

A sentence id is `s-` plus the first 8 hex digits of the SHA-256 of the whitespace-collapsed text with its links
removed, so rewording changes the id and a link edit does not. Identical text twice in one summary is a duplicate-id
rejection. Link targets classify against the summary's own location as `Source { path, anchor }`, `Sentence { summary,
id }` (a summary's sentence, written `<relative path to the summary>#s-xxxxxxxx`, the summary's own included) or `Unresolved`. A plan summary
cites a stage as `PLAN-x.md#stage:<id>`. `scan_links` is linear: its `)` search is guarded by `line.rfind(')')` so a long
run of `[` stays O(n).

## States

`evaluate::evaluate` judges every expected id against the loaded `Project`; document summaries first, so an overview
sentence can ask whether the member sentence it cites is current (`evaluate/cite.rs`).

| Level | State | Meaning |
| --- | --- | --- |
| Sentence | `current` | it has at least one citation and every citation holds: the cited span's hash equals the `cites` entry, the cited member sentence exists and is current, or the cited sentence is the summary's own, cites no own sentence and is itself current (`evaluate.rs` `own_current`, `cite.rs` `Context::with_own`) |
| Sentence | `stale` | a cited span changed or vanished, a cited member sentence is gone or stale, or a self-link goes to a stale sentence (opening sentences citing each other read stale) |
| Summary | `missing` | no file, or a file that fails validation, holds conflict markers, or names another source; checked first, so a claim on a missing summary reads `missing [claimed by ...]` |
| Summary | `rebuilding` | a claim names it and the file is valid; its sentences keep their states |
| Summary | `stale` | any sentence stale |
| Summary | `incomplete` | all sentences current, but a document's `file` hash differs from its source's, a plan summary's `bundle` map differs from `evaluate::bundle_hashes` of its briefs now (a brief added, removed or edited; `evaluate/cite.rs` `Context::complete`), or an overview's `members` map differs from the members' hashes now. A cited brief span that changed stales its sentence (`Context::holds`, `Kind::Document.bundle`) |
| Summary | `current` | none of the above |

A summary is out of date in the UI when it is stale, incomplete or missing, but the server rebuilds only the stale and
missing ones without a press: `SummaryState::rebuilds_in_background` (`evaluate.rs`) names them. An incomplete summary
says nothing its sources contradict (it only leaves out text no sentence cites), so it waits for a press or for a later
stale. Every hash a summary records or compares goes through `settle::unsettled` (`planning/summary/hashes.rs`):
`SourceHashes` is a stateless view over the loaded `Project` that gives the sections and the file hash of each document
over its text without Settled lines: `ProjectDoc::unsettled` (`planning/project.rs`) holds the sections of a document
that has Settled lines, computed once when the document is loaded or refreshed (`planning/project/parse.rs`), and any
other document keeps the loaded sections. `input_hashes(project, evaluation)`
gives one hash per summary over what it is built from. Evaluate (`cite::Context::new(&SourceHashes, ...)`), seal
(`Rules.hashes`, `document_front`) and the engine's `rewrite_one` all hash through it, and `cite::overview_inputs`
serves it and `seal/links.rs` alike, so settling or unsettling a document changes no summary state
([Settled Line](planning-trace.md#settled-line), [the mistake](../mistakes/planner-ui-repair.md#settling-a-document-invalidated-its-summaries-and-rebuilds-started-on-every-batch)). A deleted source drops its member summary and stales the overview
sentences citing it: seal requires every member cited, so the member-removed route to `incomplete` is reachable only by a
hand edit (`engine/tests.rs` pins the stale direction). A summary file that cannot be read (symlink, invalid UTF-8, oversize)
counts as missing for evaluation. `Evaluation.orphans` lists store files no expected id owns.

## Reads, Writes and the Lock

Every read of the store is no-follow and bounded (`write::read_summary`, `fs::safe_read::read_to_string_bounded`), never
`discover::read_doc`: a symlinked summary is a hand edit and counts as missing. Every write goes through `safe_fs` inside
`lock::with_store_lock`, to `.<name>.tmp-<pid>` beside the target, renamed over it; the leading dot keeps the watcher and
discovery from reporting the temp file. A planted symlink anywhere under the store fails the operation.

`summaries.lock` is one exclusive `flock` serialising the engine, planning sessions and a terminal `seal`. A `flock`
belongs to the open file description, so the lock is NOT re-entrant: `write_summary_if` runs its `build` closure under it,
and a build (seal's) that calls `with_store_lock`, `write_summary` or `delete_summary` blocks forever. `lock_exclusive`
has no timeout, so any process holding the file stalls seal, brief and the engine.

`write_summary_if(root, paths, id, expected_file_hash, build, own)` re-reads the file under the lock and writes only when
its `normalized_hash` equals `expected_file_hash` (`None`: absent); otherwise it returns `WriteOutcome::Changed`. The write
functions are `pub` although only the module uses them: `pub(crate)` ones with no caller trip `dead_code` under
`clippy -D warnings`. `OwnWrites` (`planning/watch.rs`) registers the write before the rename so the watcher does not
report it. `Watcher::poll` delivers no batch when the only change is a registered own write, so a test of "own writes
are not reported" must add an unrelated `doc/` change in the same window.

## Seal

`loom plan summary seal <id> [--force]` (`seal.rs`): validate the id, load the `Project`, require the id to be expected,
read the draft `drafts/<id>.md` from the runtime directory (bounded to 256 KiB, no-follow), check the claim, parse the
draft, then in one hold of the store lock check every link and write the canonical file. Rules (`seal/links.rs`, `seal/levels.rs`):

- every link must resolve; a `Source` link must name a source of this summary (a document: its own source, and for a plan summary also each file of its brief bundle, `Rules::allowed_sources`; `product`: the
  vision; `engineering`: the metaplan) and an existing anchor, and records its span hash under `cites`. A plan summary must cite every brief with Markdown text (`uncited_briefs`; refusal `brief <path> is not cited` on line 1); a brief without text is allowed as a source but not required, because it has no citable anchor. The plan's locked state does not matter: briefs of an `IN_PROGRESS-` or `DONE-` plan keep their sections and content hash;
- a `Sentence` link is allowed in two places. An overview may link only to a member, only to a current sentence of a non-missing
  member, and only to a **citable** sentence: one that cites no summary sentence (`links.rs` `citable`). A plan summary's opening sentences cite their own
  detail sentences, so an overview cites the detail sentences of a plan and a member sentence that cites a sentence is refused ("opens to detail sentences; cite those detail sentences
  instead"). A plan summary may also link to its own detail sentences ([two levels](#two-levels-in-a-plan-summary-seal)). Any other document summary cites only its source (a `stage:` anchor counts as a source anchor);
- an overview sentence may not restate the member sentence it cites (`seal/restate.rs`, called from `links.rs` as `restate::restates(`): opening it would show the same text twice. The measure is the Dice similarity `2|A∩B| / (|A|+|B|)` over the lowercase ASCII letter-and-digit word sets of the two texts (links removed), rejected at `RESTATE_THRESHOLD` 0.75 or above against any single cited member; an empty text restates nothing. The threshold was measured over the 103 resolvable overview-to-member pairs stored at the plan's HEAD: 2 were byte-identical, 18 scored 0.75 or more, and a reworded copy scored 0.78 ("File mode adds the server's document, tree, save, diff and search routes, with in-process plan validation." against the member's longer sentence), while a two-member compression scores under 0.75 against each member and passes. A sentence citing one member twice reports one restatement (`check()` keeps a per-sentence set). Known limit: the word set is ASCII-only, so non-ASCII prose is never flagged. Overviews sealed before the check keep their near-copies until the next rebuild;
- **two-member rule** (`single_member_problem` in `seal/links.rs`): an overview sentence that cites member sentences draws on at least two of them, because a sentence that opens to one member sentence shows the reader the same thing twice. The seal refuses a sentence whose problem-free citations name exactly one member sentence ("an overview sentence draws on at least two"), unless that member's summary holds exactly one citable sentence, where there is nothing else to cite (the exemption counts citable sentences, so a plan summary with one detail sentence can be cited alone; its opening sentences cite a sentence and are not citable). The brief's overview rule (`brief.rs`) pins the text "holds a single sentence"; the agent and codex prompts and `section_04` say "a single citable sentence". Measured over the stored overviews before the rule: 7 of 25 member-citing sentences in `feat-planner/product` and 12 of 27 in `feat-planner/engineering` cited exactly one member sentence; stored overviews change on their next rebuild. Known gap: `drawn` counts only problem-free citations, so a sentence citing one current and one stale member sentence also gets this error beside the stale error ([follow-up](../concerns/planner-ui-repair-followups.md#planner-review-fixes-unchecked-here-and-reviewer-suggestions));
- an overview must cite every member and every source document at least once, or the seal is refused;
- the front matter is computed, never taken from the draft: `file` from the source's content hash, `members` from each
  member's `members_hash` at seal time, `bundle` from `evaluate::bundle_hashes`.

### Two levels in a plan summary (seal)

`seal/levels.rs` `check` runs only for `plans/<slug>` ids. Opening and detail are told apart by the self-link alone; a one-level draft reports only rule 1 plus the caps. The rules, each a refusal with the sentence's line:

1. the opening level exists: at least one sentence cites a detail sentence. Sentence ids are hashes an agent cannot compute, so every refusal that needs them lists the draft's detail sentences as `detail sentences: s-… "first six words", …` (or `detail sentences: none`), and the agent writes the detail level, seals, and copies the ids from the refusal;
2. the opening level comes first: no opening paragraph or heading after the detail level begins, and no paragraph mixes the levels;
3. an opening sentence cites only detail sentences, never the plan's source beside its self-links (only `Source` links count; `links.rs` keeps reporting other link kinds);
4. every self-link goes to a detail sentence of this summary;
5. no detail sentence is cited twice by one opening sentence, and an opening sentence opens whole detail paragraphs in order (a run of consecutive detail paragraphs, in document order);
6. an opening sentence does not restate a detail sentence it opens (the `restate` measure);
7. every detail sentence is opened by some opening sentence;
8. at most `PlanCaps` opening and detail sentences (8..12 and 15..60, by source size).

An unknown self-link target is draft text, so the refusal wraps it in `terminal_safe` where the message is built (`commands/plan/summary.rs` applies it again to the whole line; idempotent). Known limit: the seal lets two opening sentences open the same detail paragraph; non-adjacent ones repeat the detail text when both open ([follow-up](../concerns/planner-follow-ups-followups.md#summary-levels-and-fragments)).

The seal retries up to 3 times when the file changes between its read and its locked write, then fails with "kept
changing". On success it deletes the claim first, then the draft; a draft-removal failure is logged with `tracing::warn`
and the seal still succeeds. Resealing unchanged content produces no diff (`render` is canonical). A rejection is
`SealError::Rejected` with one entry per problem and draft line (CLI exit 1); I/O failures are `SealError::Io` (exit 2).

## Claims

A claim marks a summary as being rebuilt, so two sessions do not rebuild one (`claims.rs`). It is
`claims/<id with '/' replaced by "__">.json` in the runtime directory, created with `O_EXCL`: the create failing because
the file exists IS the refusal. It records the summary, the session (`LOOM_PLAN_SESSION`, sanitised to 128 characters with
every control character stripped; absent from a terminal), the pid and `created_at`. `claims/` is session-writable, so
reads are no-follow and bounded (4 KiB), and a claim that cannot be parsed still counts as a claim on its file-name id.
`create_claim_as` retries the create once when the existing claim vanished between the failed create and the read, and
errors rather than fabricating a terminal claim; `ClaimHeld` is the typed error the CLI downcasts to exit 1.

`loom plan summary brief <id>... [--note <text>]` de-duplicates the ids, claims all (releasing earlier claims if a later
one is held), and prints the rebuild instructions with the draft path; each current sentence is marked `[keep]`.
`seal` accepts a claim of the caller's own session, or any claim from a terminal with `--force`; a planning session never
forces. `release <id>...` deletes claims; `status [--json]` lists every summary with its state, sentences and claim
(`claim` key, `session` null for a terminal claim; `render_status` passes `claim.session` through `inline_safe`).

## Engine and Rename Rewrites

`SummaryEngine::on_batch` (`engine.rs`) runs once per watcher batch under the server: rewrite renamed sources, delete
orphans, evaluate with the live claims, and report the summaries that newly went out of date (`invalidated`).

A document summary is rewritten when its `of` is an earlier path of its source (`evaluate::same_source`: for a plan, any
`doc/` path classifying as a plan of the same slug) and its `file` hash equals the source's current hash. The rewrite
(`with_source`) moves `of`, the `cites` keys and every body link naming the old path, recomputing the links relative to the
summary, and goes through `write_summary_if` with the file's own hash and the shared `OwnWrites`. The batch's `renamed`
list is only a hint and is not read, so a rename made while no server ran is also caught. A failed rewrite or deletion is
logged and retried by the next batch.

## Configuration

Summary models resolve per key, project tier then user tier then default (`summary/config.rs`, `SummaryModels::resolve`).
See `conventions/model-and-effort-config.md` (`[plan]` Keys and Defaults).

## Tests

Contract tests: `loom/tests/planning_summary_contracts.rs` (harness `tests/planning_summary_support/`) and
`planning_summary_cli_contracts.rs`. A mutation run broke each contract's rule and confirmed red. Two contracts are weaker
than they look: `overview-members-ignore-link-edits` stays green when `members` is not hashed at all, because rewording a
member also stales the citing sentence; the symlinked-store and id-traversal contracts go red only when every guard layer
is broken, since the layers back each other up. `summary_cli_uses_session_state` needs a non-root uid (the runtime check
uses `access(W_OK)`). Tests that mutate `LOOM_HOME` carry `#[serial]`, which excludes only other serial tests.

## Reachability Gotchas

The `reachable` check's source graph does not resolve a module-path call such as `seal::seal(...)` through a module
import: import the function directly or call it by its full `crate::` path, as `dispatch` does for `check::execute`.

The graph also drops every call edge to a method name with more than `MAX_CANDIDATES` (8) definitions
(`loom/src/context/source_graph/mod.rs`), and the `SessionSpawner` trait declaration has no body, so it is not a node. A
ninth test stub named `start_summary` silently cut the dyn-call path from `install` to `start_summary`. Reuse one
`SessionSpawner` stub across the tests and keep `start_summary` definitions at 7 or fewer.

## Rebuilds and Cost

The planning server rebuilds stale and missing summaries on its own, once their sources have been quiet for 5 minutes, one summary session at a time, and logs what each
rebuild spends. Code: `loom/src/plan_server/summaries/` (`scheduler.rs` and `scheduler/` hold the pure rules,
`rebuilds.rs` and `rebuilds/` the facade, `spawner.rs` the real session start, `attribution.rs` claim attribution and
cost lines, `routes.rs` the four `/api/summaries/{panel,rebuild,release,answer}` routes) and
`loom/src/planning/summary/cost.rs` with `cost/estimate.rs`. Web: `web/src/plan/summaries/`.

- **Pure scheduler, two-phase facade.** `Scheduler` does no I/O and reads no clock: every call takes `now` and a
  `SessionSpawner`. The facade (`Rebuilds`) runs each call twice over: under its lock the scheduler talks to a recording
  spawner that answers a pending id (`(starting)`); after the lock the facade calls the real `start_summary` or `stop` and
  settles the id with `Scheduler::resolve_start`. The lock cannot be held across `start_summary`, because `start_session`
  flushes bridge events synchronously into the facade's own bridge listener. An `Ended` that arrives before the settle is
  kept in `State.ended_early` and applied after the start is published.
- **Locks.** `Attribution` has its own mutex beside the scheduler state (it deletes claims and appends cost lines, both
  file writes); the two are never held together. `publish` holds `panel_sent` across the question broadcasts and reads
  the current question under the state lock, so the lock order is `panel_sent`, state, log.
- **Install order.** `summaries::install` needs the chat's `Bridge` in `ctx.extensions()`, so its line in
  `routes.rs` `install_all` follows `chat::install`; installed earlier it prints `summaries: no rebuilds` and never starts a
  session.
- **Concept pages.** `publish` computes the pages to refresh from the evaluation's `Scope` through
  `concept::delta::page_keys`, so it needs no `Project` and takes no project lock. A `ConceptChanged` broadcast follows every
  rebuild state change.
- **Quota.** The panel reads the quota cache through `crate::quota::read_snapshot(runtime)`, the file the shared
  poller writes (`architecture/quota-poller.md`); the summaries add no poller of their own. The read runs under
  `panel_sent`, so a cost record from the bridge's flushing thread waits behind a panel build.

## Rebuild Scheduler

Source: `plan_server/summaries/scheduler.rs`, `scheduler/queue.rs`, `scheduler/session.rs`.

- **Limit.** `AUTO_LIMIT` is 5. `try_start` starts the whole queue when it holds at most 5 ids or every id skips the
  limit. Otherwise it starts the Button and approved ids alone, the front of the queue, and holds the batch ids. With no
  such ids it opens one question (`q<n>`) naming the whole queue and starts nothing until it is answered. A first
  evaluation counts like any other, so a repository with more than five missing summaries asks first. Yes promotes the ids
  as approved and starts them with no second check; a Button press never asks.
- **Auto-rebuild switch.** `POST /api/summaries/auto {enabled}` (`summaries/routes.rs`; holder-only, refused in read-only mode) sets the scheduler's start-without-asking limit to `AUTO_LIMIT` or 0; with it off every background batch opens the question first. The Settings toggle "Rebuild summaries automatically" is a per-browser preference (`web/src/plan/summaries/state.ts`) that the holder's browser posts after `hello` and on every change, because a browser-only preference cannot change a server-side rebuild.
- **Newly invalidated counting.** `on_update` queues only the `invalidated` ids (newly stale or missing in this batch,
  by `rebuilds_in_background`; `engine.rs`), minus
  the declined, the running session's pending ids and the queued ones, and first drops every id no longer out of date from
  the queue, declined, failed, unsealed and question sets, except a pressed id whose summary the latest evaluation still
  lists (`Scheduler::set_known`, fed by `Rebuilds::apply_evaluation`): a press rebuilds a current summary too, so the
  batches that arrive while it waits must not take it away. An id missing from the known set is dropped, so a pressed
  id whose document was removed cannot restart sessions without bound. A question that loses ids is re-sent with the rest and closes
  when none remain. The ids already out of date when the server starts, or when a turn or suspension ends, enter through
  `refresh`. At an authoring turn's end (`turn_ended`, `chat_idle` in `rebuilds/session.rs`) that set comes from a fresh
  evaluation of the store (`Rebuilds::to_rebuild_now`, `rebuilds/evaluation.rs`), made outside the state lock and never
  stored in `state.latest`. A chat can seal a summary inside its turn (the correction rebuild of product spec section 10,
  "When the summary is wrong"), and the watcher batch showing that seal can arrive up to 1 s after `TurnEnded`: the cached
  set would queue what the turn just sealed, and a summary with no change on record would start at once. A pressure run's end and a
  `SuspendGuard` drop still resume on the last batch's set.
- **Quiet period.** A background rebuild waits for the edits to stop. `QUIET_PERIOD` is 5 minutes (`scheduler/config.rs`;
  not a `SchedulerConfig` field, because the frozen `plan_summary_rebuild_contracts.rs` builds that struct by literal),
  the rules are in `scheduler/quiet.rs`. The facade diffs `summary::hashes::input_hashes` on every batch
  (`State::note_inputs`, `rebuilds/evaluation.rs`; the first record is a baseline and reports nothing) and calls
  `Scheduler::sources_changed(ids, now)`. A Batch id then waits until 5 minutes after its last change, measured from the
  end of the authoring turn for an id changed while a turn was open (`restart_turn_changes` in `turn_ended`). `tick`
  (`wake_quiet`) starts what becomes ready, and `try_start` considers only ready ids for starting, for the limit and for
  the question. A press and a Yes skip the wait, and an id with no recorded change (out of date at startup) does not wait.
  The wire panel lists the queued Batch ids inside their quiet period as `waiting` and the incomplete summaries nothing
  queues, runs or failed as `incomplete` (`wire/summaries.rs`, `rebuilds/panel.rs`); the planner shows them as the
  sections "Waiting for edits to stop" and "Incomplete" with Rebuild rows, the status pill counts an incomplete summary
  as out of date, and `rebuild-button.tsx` `useQueued` ignores waiting ids so a press still starts. Limits: a waiting
  summary's concept heading reads "Out of date: the source changed" (the concept wire carries no waiting reason). Quiet
  records (`since`, `in_turn`) are pruned only when their summary leaves the evaluated set (`Scheduler::known`, through
  `prune`) and expire after `QUIET_PERIOD`, so a claimed (Rebuilding) summary edited mid-session still waits after an
  unsealed end; `scheduler.rs` and
  `rebuilds.rs` are about 396 lines each, at the 400-line limit, so the next change there splits a file first.
- **Decline memory.** No to a question moves its ids to `declined`. A declined id is skipped by `on_update` and `refresh`
  until a Button press (`forget` clears declined, failed and unsealed) or until it stops being out of date. A failed id
  is skipped by `refresh`, and `on_update` clears the failure of an id that is newly invalidated.
- **Suspension guards.** Nothing starts while any of these holds: `read_only` is set, the server is stopping, a
  suspension token is live (`AuthoringTurn` from `turn_started` until `turn_ended`, or the authoring session going Idle
  or Ended with its turn open; `PressureRun`), or a session is running. Releasing the last token runs `refresh` and
  `try_start`. An open question holds only the batch: pressed and approved ids start beside it, and `Scheduler::press`
  takes its ids out of the question (`retain_asked` re-asks the rest or closes it).
- **One session at a time.** `try_start` returns while `running` is set. A start that errors keeps the ids queued and
  shows the error in the panel until the next start.
- **The ends of a summary session.**
  - *Claims done* (`claims_changed`): the session stops with the claims kept (`stop(session, false)`) once every id was
    seen claimed or sealed and no claim is held. The rule is "seen", not "at least one", because the brief rebuilds the
    documents in subagents and claims the overviews afterwards: an empty listing before every id was claimed is a gap.
  - *Stalled* (`tick`): no seal within `NO_SEAL_TIMEOUT` (10 minutes, reset by each `seal_seen`) stops the session with
    its claims released; the unsealed ids fail with the last seal error, or `no seal in 10 minutes`.
  - *Ended* (`session_ended`, `session_failed`): the session ended on its own. Unsealed ids requeue at the front of
    their part of the queue; with a failure reason they fail with it. The server stopping clears the queue and question
    and only reports the stop; the bridge drain ends the session.
- **Unsealed twice.** An id the session claimed and left unsealed at a normal end is marked; a second such end moves it to
  `failed` with `ENDED_UNSEALED`, so a summary whose seal never validates does not rebuild forever at a token cost per
  round. A seal, a Button press or a failure clears the mark; a prune clears it unless `keeps_mark` holds (the id is
  queued, or pressed and pending in the running session, or claimed by it: a pressed id is current all along and a
  claimed one reads `Rebuilding`, so being listed current says nothing about them, and clearing the mark there would let
  each claimed-unsealed end requeue without bound). `apply_evaluation` calls `claims_changed` before `on_update` so the
  prune sees a claim the evaluation is first to show. An id the session never claimed requeues and keeps its mark. Limits: a session that keeps ending without claiming anything restarts without bound, and `on_update`
  clears `failed` but not the mark of a newly invalidated id, so after a source edit one more claimed unsealed end fails it.
- **Press path** (`rebuilds/tests_press.rs`: `a_pressed_rebuild_reaches_the_spawner_within_one_tick`, `a_pressed_session_is_costed_as_button`, `a_press_during_a_running_session_joins_the_next_start`). An idle press or a Rebuild-all answer runs `Rebuilds::enqueue(Button)`, `Scheduler::press`, `try_start`, `carry_out` and `start_summary`; `Bridge::start_session` returns with the session Spawning and `host.spawn` runs at the next driver tick (20 ms in the test rig, under 1 s, no other event needed), so the bridge, the 200 ms tick and recovery add no delay. What defers a press: a running session (one at a time; the panel names it, and the pressed ids join the next start at the session's `Ended`, after a claims-done stop or its own end), an authoring turn or pressure run (the panel shows only the queue until the turn ends), and a bridge refusal during recovery (`Busy` keeps the ids queued with `start_error` until `after_recovery` supplies a batch). An open question about a background batch does not defer it.
- **Claim clearing.** `Attribution::on_session_ended` deletes every claim the session holds; a stopped subagent's claims
  on the ids it briefed are deleted first. `sweep_after_recovery` deletes the claims of sessions that are neither live
  nor left alone and returns the terminal (session-less) claims, which stay and are offered for release in the panel. The
  facade applies a claims listing from its 5 s tick only when two consecutive listings agree: a seal deletes its claim up
  to a second before the batch shows the sealed file, and acting on one listing could requeue a summary that is already
  sealed. The batch path applies claims from the evaluation at once.

## Cost Ledger and Estimates

- **Ledger.** `summary-cost.jsonl` in the planning runtime directory (`RuntimePaths.summary_cost`) holds one JSON
  `CostLine` per record: time, trigger (`refinement`, `background`, `button`), the summary ids sharing the cost, level
  (`document`, `overview`), model, source bytes and a `TokenVector` of fresh input, cache write, cache read and output.
  An absent component means unknown usage, never zero. `append` writes a complete line under an exclusive lock on a
  no-follow open; `read_log` reads the last 8 MiB (`MAX_LOG_BYTES`), drops the partial first line and skips malformed or
  unfinished records, so older records fall out of estimates and totals. Known gaps: `append` has no `O_NONBLOCK` or
  regular-file check, so a FIFO at the path blocks the open under the attribution lock, and `read_tail` drops a complete
  first record when the cut falls exactly on a record boundary.
- **Accumulator reuse.** `transcript_tokens` feeds the complete lines of a transcript to `AgentState::new(AgentKind::Main,
  path)` through `tokens::tail::read_lines` and reads `state.total`. It is the token tracker's parser, so message-id
  merging and the synthetic-model filter match the dashboard's figures; an unreadable transcript is an all-`None` vector
  and an unfinished final line is skipped.
- **Per-summary versus per-batch attribution.** A subagent whose transcript ran `loom plan summary brief <ids>` is bound to
  those ids (`attribution/scan.rs`); when it stops, its tokens are one line for the ids it briefed. A summary session's own
  transcript, costed when it ends, is one line for the ids no subagent briefed (the overviews it built itself), else for
  all of them. A line with one id is an individual cost; a line shared by several cannot be split, so the turn footer shows
  tokens only for single-id lines and `tokens unknown` otherwise. Every line of a summary session is Button when any of
  its ids was pressed or approved through a question (`scheduler/session.rs::running_triggers` maps `Via::Button` and
  `Via::Approved` to `Trigger::Button`; `attribution.rs::pressed_or_background` reads the session's triggers), else
  Background: background ids that joined a press ran because of it, so its lines cannot be split by id. Refinement
  subagents log Refinement.
- **Estimates.** `estimate` sums one figure per pending summary, counting fresh tokens (cache reads excluded) and the
  weakest basis used: `Last` (the summary's latest single-summary line with every fresh component known), else `PerByte`
  (the median tokens-per-source-byte of single-summary lines at the same level, rounded up), else `Starting`: 40,000
  tokens for a document (`DOCUMENT_STARTING_TOKENS`) and 150,000 for an overview (`OVERVIEW_STARTING_TOKENS`). These
  starting figures are placeholders until the ledger holds real lines. `totals` sums known components per local calendar
  day and over the trailing 168 hours, excludes records dated after `now`, and leaves a component absent when no record in
  its window knows it. Arithmetic saturates at `u64::MAX`.
- **Measured cost: none.** No operator-run rebuild of this repository's summaries has been made, so no token cost is
  recorded and the starting figures are unchecked. Integration-verify deferred it: headless Chrome, a live tmux probe
  and a real summary session cannot run in the stage sandbox. The scheduler, the facade over a fake bridge host, the
  routes on loopback and the web components are covered by tests; the first operator run should append its figures here
  with the date and the project's size.

## Summary Backends: Claude and Codex

`plan.summary_backend` (`claude` default, `codex`; keys in [model and effort config](../conventions/model-and-effort-config.md#plan-keys-and-defaults)) picks who builds summaries and overviews. `summaries::install` installs one `BackendSpawner` (`summaries/backend.rs`); it reads `SummaryModels::load(root).backend` per job and routes `start` to the Claude `RealSpawner` or to `CodexSummarySpawner` (`summaries/codex_spawner.rs`, with `codex_spawner/registry.rs` and `guard.rs`), and routes `stop` to whichever owns the session id. Codex ids carry the prefix `c0de-` (`SESSION_PREFIX`, `is_codex_session`); an issued id must also pass `planning/runtime.rs::check_state`'s hex-and-dash test, because every `loom plan summary` call inside the job validates it.

- **A Codex job is not a bridge session.** The scheduler talks only to `SessionSpawner` and bridge `Ended` events, so the trait gained `runs_outside_bridge(session)` (default `false`; Codex answers `true` for live jobs, and `check_alive` skips them). `Rebuilds::with_spawner(ctx, spawner)` builds the rebuild state over a chosen spawner and `Rebuilds::codex_session_ended(id, outcome)` clears claims and attribution and calls `State::summary_ended`, like `session_gone`. `summary_ended` picks the Codex path by `is_codex_session(id)` because `ended_early` replays a job's early end through it, and a clean exit that beats the start settling must still reach `Scheduler::job_ended`.
- **Command line.** `codex_summary_args` keeps `--sandbox workspace-write` (never `danger-full-access`) and grants only `<runtime>/drafts` and `<runtime>/claims` with `--add-dir`; the rest of the runtime directory (other sessions' permission answers, settings, launch scripts) stays read-only. The model and effort come from `plan.summary_codex_model` and `plan.summary_codex_effort`. The prompt runs a bare `loom plan summary ...`, so it relies on `loom` being first on the server's `PATH` (hooks use `spec.loom_bin` instead).
- **Store lock under the grant.** The job's grants exclude `summaries.lock`, so `planning/summary/lock.rs::open_lock` opens it `O_RDONLY` and falls back from `EROFS`/`EACCES` on `O_RDWR`; `flock` works on a read-only descriptor. It would not serialise on an NFS home.
- **Guard.** `guard.rs` snapshots `.claude` (the `config_guard` walk), `.codex` and the Codex instruction files at the project root at job start and fails the job's ids when any changed or became unreadable by exit. It compares start with exit only, so an edit by the user or the authoring session mid-job also fails the job.
- **Shutdown.** The summaries drain waits up to 5 s for running jobs and SIGKILLs what remains; the server's abort hook SIGKILLs Codex groups itself, from a `stopped_groups` list kept in the registry after a job leaves it, because `Shutdown::abort_all` ends in `exit(130)` right after the aborts return and a sleeper thread would not survive it.
- **Opening submission (Claude).** A summary opening that is typed but never submitted is fixed in the bridge, not here: [Delivery](planning-session-bridge.md#delivery).

No live `codex exec` has run this backend: a stage sandbox has no network and refuses nested `bwrap`, so the sandbox-write, `check_parent` and `O_RDONLY` paths rest on tests and reasoning ([follow-ups](../concerns/planner-ui-repair-followups.md#codex-summary-backend)).
