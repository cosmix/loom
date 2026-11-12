# Planning Session Sandbox Limits

## Planning Session Sandbox Limits

Open items of [Planning Session Sandbox](../architecture/planning-session-sandbox.md). The accepted
limits are the ones `doc/product/feat-planner/section_12.md` lists ("Bash writes inside `doc/loom/`", "New
top-level paths", "Locked plans locked mid-session"); the implementation matches them.

### Accepted limits the implementation confirmed

- **Bash writes inside `doc/loom/`.** `loom knowledge` and `loom plan summary` need OS write access
  there, so a raw approved Bash command can write the same bytes.
- **New top-level paths.** Layer 1 denies only the top-level entries that exist at spawn
  (`filesystem.rs` `authoring`), and the root directory stays writable. An approved command can create
  a new root file; the sharpest instance is `<root>/.ignore` holding `!.loom`, which un-ignores the
  state root's `terminal-history` for a repo-root Grep (the state-root `.ignore` lists only
  `STATE_ROOT_SECRET_FILES`).
- **Locked mid-session, for Bash.** Layers 1 and 2 are built at spawn; only the guard rescans the
  locked set, and it covers file tools. An approved Bash command can write a plan locked after spawn.
- **Check-to-write symlink swap.** A symlink swapped in between the guard's decision and Claude Code's
  write passes (`guard/mod.rs` `confined_file`); creating it needs a holder-approved command.
- **Search above the state root.** A Glob rooted above the state root with an ordinary pattern
  (`**/*`) is allowed and may list state-root file names with `rg --files --glob`; reads stay denied.
  The parent rule covers only the immediate parent of a state root outside the project, so a plain
  search at a higher ancestor outside any git repository walks into it, as the shell guard does.

### Unverified or untested

- **Seatbelt.** `planning/runtime.rs` `check_parent` relies on `access(2)` reporting Seatbelt's write
  deny on the planning root; no macOS run exists.
- **Provenance race.** `check_state` and `grants_mounted` are path-based: `access(W_OK)` and
  `symlink_metadata` resolve ancestors at check time and `safe_open_dirfd` applies `O_NOFOLLOW` to the
  last component only, so a parent swapped between check and use, or a self-made parent chmodded to
  0555, passes. Harden by opening the state directory as a dirfd once and `fstatat`-ing the grants
  relative to it.
- **No gate test for `grants_mounted` accepting a state directory** (it needs real mounts); only the
  srt confinement test covers it, and that test skips in a stage's criteria runner. Keep
  `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` host runs as acceptance evidence.
- **No test for the name filter on top-level `doc/` files** (`permissions.rs` `RULE_SYNTAX`, symlinks):
  a file named `*` would re-emit `Edit(//<root>/doc/*)` without failing a test. Add `doc/*` and
  `doc/link -> loom` to `no_kind_allows_an_edit_under_doc_loom`'s fixture.

### Code debt

- `sandbox/mod.rs` `write_session_sandbox` builds `guard.json` from a `json!` literal that duplicates
  `GuardPolicy` and its `SCHEMA`; serialize `GuardPolicy` directly.
- `sandbox/assert.rs` infers the kind from a `Bash(touch` allow; an authoring object carrying one
  skips the peer-tool deny check. It also accepts any hook command containing
  ` plan hook PreToolUse ` and extra `PreToolUse` entries, and checks neither `sandbox.filesystem` nor
  `network.strictAllowlist`. Pass the kind and tighten the checks.
- `sandbox/tests.rs` `expected_read_denies` duplicates `tests_kinds_fixture.rs` over a separate
  `Fixture`; consolidate. The module doc of `sandbox/tests_kinds.rs` still says the expected-value
  helpers are reused by `tests_assert`; rewording it raises a test-integrity event, because the line
  contains `tests_assert`.
- `doc/product/feat-planner/section_12.md` (graph-cache paragraph) and `context/graph_store/fallback.rs` say
  `loom knowledge context` refreshes the source graph cache; retrieval refreshes structurally and a
  denied write falls back to an in-memory knowledge catalog, not a graph layer. Reword: `loom map`
  refreshes the source graph and `loom knowledge context` the knowledge catalog, both under
  `.loom/cache/context-v1`.
- `doc/plans/feat-planner/briefs/feat-planner-06-session-sandbox/planning-sandbox/w1-settings.md` states
  `read_denies(...) -> ReadDenies`; the function returns `anyhow::Result<ReadDenies>`.

## Planning Session Bridge Follow-ups

Open items on `loom/src/plan_session/` and `loom/src/plan_server/chat/`, from review rounds and the host runs. Reviewer-reported items were not re-run against the final tree unless a line says so. Settled behaviour lives in [Planning Session Bridge](../architecture/planning-session-bridge.md).

## Host verification

The operator end-to-end is unverified and its last run failed (opening delivery timed out at 120 s with
state `Idle`); it is not to be rerun as a direct unsandboxed host test. Diagnosing the stall needs a
sandboxed tmux environment or a separate post-merge investigation. Evidence to start from:
`loom/target/tmp/plan-session-e2e/1793208662-the-opening-s-delivery/` (report, pane, spool, transcript) and the
three tmux tests that print SKIP inside the stage sandbox.

## Teardown and abort

- `ending.rs` `teardown_socket` returns Ok when the socket file is missing, so a tmux server whose socket was
  deleted keeps `claude` alive; verify the recorded identity after teardown and `terminate_verified` it.
- `retry_teardown` claims a `TeardownFailed` session without changing its phase; a concurrent abort can tear
  the same session down twice (double `kill-server`, directory removal). Setting `Phase::TearingDown` in the
  claim makes it exclusive. A recorded process that is `Dead` is never retried although servers run
  `remain-on-exit`.
- A no-pid `TeardownFailed` session against a stuck tmux server retries every `identity_interval`, each try
  blocking the driver up to the 5 s `kill-server` bound; a backoff would protect other sessions' ticks.
- `Bridge::abort` tears live sessions down synchronously before its 30 s `LAUNCH_WAIT`, so its total can pass
  30 s; the `ShutdownHook` doc (`plan_server/shutdown.rs`) says "must return fast" without the bound.
- `on_recovered` callbacks run without `catch_unwind`; one that panics skips the rest.
- `session.json` is rewritten in place (`registry.rs` `write_record`); a torn write reads as an unreadable
  record, which recovery leaves alone (safe). Temp file plus rename would be cleaner.
- `env -i` treats a `claude_bin` path containing `=` as an assignment; paths convert lossily.

## Turn and state races

- `press_step` should recheck `Working`/`Permission`: a Stop applied in the same tick as an accepted interrupt
  leads to an Escape at the idle prompt.
- `begin_turn` closing an open turn publishes `TurnStarted{t2}` before `TurnEnded{t1}`; `Idle` is visible under
  the bridge lock before `finish_turn` flushes its `TurnEnded` (`bridge/commands.rs`), so a test or consumer
  counting `TurnEnded` right after `Idle` can miss that turn.
- An opening whose paste succeeds but times out leaves the session paused with no turn; a `FinalWrite` end
  then pastes the closing message into it (bounded by `final_write_bound`).
- `turn_running()` is false during `Ending` while the closing turn writes documents, so a Restore can
  race the final write. `end` during `Preparing` is refused and the browser shows "the chat is closing".
- `StopFailure` is assumed main-agent only; one carrying `agent_id` would idle the main session mid-turn.
- `SessionEnd` outside `Ending` gives `Ended`, not `Failed (died)`.
- The close gate (`BackgroundTasks::blocking() == 0` in `check_close`, `bridge/driver.rs`, called from `on_idle` and each tick) is pinned: the contract `a-summary-session-waits-for-a-launched-agent` and `tests_end.rs` fail when it is dropped.

## Transcript mapper

- A human-typed string starting `[Request interrupted by user` classifies as an interrupt (`records.rs`); require
  a non-human origin. The post-compaction summary record (`isCompactSummary`) may render as a human message.
- `fallback_summary` and `tool_item` serialise the whole untrusted input before capping (`tools.rs`).
- A `Read` with only `offset` renders `path:40-40`. `turn_duration` goes to the open turn even when the next
  turn already opened; mapping by `parentUuid` would attribute it exactly.
- `ingest_tokens_only` has no `uuid` dedup; after a token-transcript reset, old usage is re-added.
- Subagent lines read before the meta file or Agent result names the spawner keep `parent: None`.
- A quote ending in a fake cut marker skips the 2,000-character cut (bounded by `MAX_QUOTE_BYTES`).

## Chat server and wire

- A new tab can receive `ItemAdded`/`TurnAdded` twice (the hub registers a connection before `on_open`; the
  bridge outbox may hold deltas already in the snapshot). `web/src/plan/chat/model.ts` appends `item_added` without checking the id, so a repeat shows twice; it needs upsert-by-id semantics or a sequence number.
- `follow_authoring` and `describe` clone the whole chat snapshot under the bridge lock to read one
  `SessionView`; a `Bridge::chat_view()` accessor would avoid it.
- `plan_server/chat/mod.rs` is 398 of 400 lines; the next addition needs a split.
- `Chat` and `Review` are matched but absent from `all_plan_messages()`/`all_client_messages()` in `wire/tests_schema.rs`;
  the web validates them through `socketMessageSchema`. Fixtures have no nested subagent row (`parent` is always null).
- A request too large to show is denied `hook_error`, the nearest reason in the closed list; the browser shows
  it as a hook fault.
- `ChatState::starting` guards `give_back` and the ended-chat release; a direct `Bridge::start_session`
  caller (plan 11) must hold it.

## Recovery

`deny_unanswered` inserts the answer even when `write_answer` fails, so a replayed card shows Denied with no
file. Recovery replay publishes `TurnStarted`/`TurnEnded` for every historical turn.

## Trust entry and input safety

- `trust.rs`: a dangling `.claude.json` symlink fails canonicalize and the fallback replaces the link with a
  file holding only the trust entry (fall back only on `NotFound`); no parent-directory fsync after persist;
  no test fails if `update_global_state` drops its `locked_dir_update` wrapper. The module doc still says the
  entries serve worktree sessions only.
- `session_settings/contents.rs` writes `hasTrustDialogAccepted` into the capsule settings file, which does
  nothing on Claude Code 2.1.312, and `tests_capsule_contents.rs` pins it.

## Paste, slash and bang input

- `paste-buffer` without `-r` turns LF into CR: if a pane has no bracketed paste, each line submits alone, so a
  slash command on line 2 of a message that does not start with `/` would run. `check_send` refuses a message
  that starts with `/` unless it is one line, and checks only that first token otherwise ([the gate](../architecture/plan-server.md#the-send-gate)).
  Whether a pasted leading `!` enters Claude Code's bash mode is unchecked (the P2 probes pasted no `!` line).
- A slash message's disguises (control characters, a leading U+FEFF or U+0085) are stripped before the first
  token is read, so they stay refused; a dialog command left open in the pane would capture the next pasted
  Enter, which is why an unlisted name is refused rather than sent.
- A skill's inline shell does not run at expansion: the expansion record carries `promptShellHandOff` and the
  model runs the commands as ordinary sandboxed `Bash` calls, so a command outside the project fails with
  `Read-only file system` (`sandboxed: true`) and the skill reports it ([probe-notes](../../../../loom/tests/fixtures/plan_session/probe-notes.md#inline-skill-shell)).
- The suggestion menu in the pane is open for a bare name and closed for a line with arguments. In all ten
  bare-name pastes the first row was the typed name (or its canonical name) and Enter ran it; a typed name that is a
  prefix of another name, with the exact name not first, was not observed, so Enter might run the other command.
- A skill's `allowed-tools` was observed only through a delegated `web-fetch` subagent whose `WebFetch`
  raised a `PermissionRequest` that auto mode's classifier accepted; a `WebFetch` made by the main agent under the
  skill's `allowed-tools` was not observed asking for permission.
- The native table is pinned to Claude Code 2.1.321 ([follow-ups](planning-session-sandbox-limits.md#planner-commands-native-table-parity)).

## Operator e2e harness

`tmpdir_round` and `permission_round` share the stale-`Idle` risk of the earlier rounds (read `before` ahead of
`send("/compact")`; wait for the turn); an unanswered `echo $TMPDIR` permission request would hold the round for
300 s; the Stop/StopFailure report reads the spool 1 s after Idle; the second message's 60 s wait includes the
first turn; the interrupt report matches only `Stop`; `diagnose.rs` finds the transcript only through
`session.json` and runs `capture-pane` twice. `config()` docs overstate that `home` is the HOME the recovery
test's `claude` reads.

## Review nits not applied

`state.rs` `last_idle_at`/`last_main_at` are public and used only by tests; the `$deserializes` wire tests are
subsumed by the `$parity` tests; the `ENDED_KEPT` doc sat on `RecoveryWaiter`; a guard test pinning the oversize
and invalid-JSON deny texts is missing; the `bridge/tests_end.rs` opening-failure test could check the session
directory and a `session_ended` deny; `hold_flush` loops 100 times against `MAX_QUEUED` 20; the comment on the
offer sits between `starting()` and `take_authoring`; the spawn-failure warning does not name the starter;
no deterministic unit test of `Batch::in_time_order`; one requests mutation note mislabels its mechanism.

## Recurring flock-after-drop

`ServerLock` (`plan_server/lock.rs`) is the third lock type found holding its `flock` past `drop` under a
forking test binary ([mistake](../mistakes/test-concurrency-and-fixtures.md)). A proposal: a maintainability
test that fails for any struct owning a locked `File` without a `Drop` that calls `flock(LOCK_UN)`.

## Bridge test caveats

- `a_launch_whose_claude_exits_at_once_is_torn_down_and_reported` (`launch_tests.rs`) relies on the stub `claude`
  exiting before `spawn_in_tmux` finishes its `has-session` and three `set-option` calls; if it flakes, the
  identity read won the race and `launch` returned Ok.
- The three tmux launch tests print SKIP inside the stage sandbox (no unix sockets) and still count as passed;
  only `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` turns a skip into a failure.
- The chat dispatch race tests start the second tab's chat through `Bridge::start_session` directly; two
  dispatches cannot be ordered without timing because nothing blocks between `take_authoring` and the reserve.

## Planner commands: native table parity

- The native command table (`plan_session/commands/builtins.rs`) is pinned to Claude Code 2.1.321. A native command a later version adds reads as a bundled skill when the headless list marks it `builtin: true` (class Run, so a native dialog command would be pasted and could capture the next Enter), and as unlisted (refused) when the list omits it. The menu matches the terminal for that version only. Detection: after each upgrade rerun `p2_side_process_initialize_and_commands_changed` ([probes](../architecture/planning-session-bridge.md#verification)); a startup warning when an unknown `builtin: true` row's description reads like a native command would surface the drift (`commands/list.rs:169`), not built.
- `loom-ui` is disabled by the session settings; whether an installed and enabled `loom-ui` lists `/loom-agents` was never observed (not installed in the probe HOME). A rerun that lists `/loom` or `/loom-agents` despite the setting adds them to the native table as Terminal only.
- Listed rows named like a native alias are hidden (fail closed, `list.rs:115`). If Claude Code resolves a skill before a native alias, such a skill is unreachable from the planner; native precedence is unverified.
- A row name `inline_safe` changes (whitespace, backtick, over 200 characters) is listed but refused as unlisted when sent (`plan_server/commands/mod.rs:73`); consider hiding such rows.
- Not observed in P2: `/stats`, `/effort auto`, `/effort` with no argument, a confirmed `/model` switch, the project-skill description marker, an alias typed in place of a name, and a slash line of exactly 20 characters or over 300.

## Planner background table open items

- An agent with no transcript facts, or whose last row is a bookkeeping row (fold `Unknown`), never stalls and keeps `blocking() > 0` until a `Stop` list, a notification or a `SubagentStop` ends it (`bridge/background.rs:116`). The next `Stop` list that omits it ends it, so the exposure is one turn; a stall fallback on `last_activity_at` would bound it.
- An unknown-id notification takes its kind from the summary wording (`starts_with("Agent ")`, `background/inputs.rs:173`); an unknown agent with another or no summary shows as a shell.
- The module doc's read-order independence holds for ranked fields and times. `state` has no rank and depends on read order through reopen and end, and a `toolEndsTurn` line or hand-back read before any launch evidence is not remembered in `AgentFacts` (unreachable with today's feed order, `registry.rs:290-309`).
- `clear()` keeps the agents map, and each `AgentProgress` keeps its last raw line `Value` (up to 32 MiB per agent) for the session's life; the fold is stored, so `last` could be dropped after `agent_line`. Each agent line is parsed three times on the driver (`stamp_of`, `ingest_agent`, the mapper) and deep-cloned by `observe` (`agent_state/progress.rs:115`).
- Ended tasks stay until a clear, and every changed tick broadcasts the full table to every tab (up to 5 frames a second while an agent works), so a long session with many finished tasks makes large frames (`bridge/driver.rs:292`); the web `entries` map in `chat/background-tasks.ts` keeps one `selectAtom` per `tool_use_id` for the tab's life.
- `read_agent_meta` runs for every subagent on every 5 s pass on the `planner-tokens` thread (`chat/background/groups.rs:24`); the bridge `Feed` already holds each agent's teammate flag and type. The literal `"tokens"` in `chat/background/mod.rs:108` is under the plan server runtime directory while `TOKENS_DIR` names the work root's; no collision.
- `paused` is cleared by every local settle (`bridge/local.rs:241`), but it is only set with the slot empty, and only a late local or a fork refills a paused slot. A failed `/compact` that writes no output record still holds the slot (a settling local is exempt from the delivery timeout); consider an upper bound on a confirmed compact with no boundary and no output (`local.rs:268`).
- `commands/side.rs:232`: `process_group(0)` keeps the side process out of the server's terminal signals. If the server is SIGKILLed cleanup never runs and `side-settings.json` (0600) stays in the session directory; delete a stale one before writing.

## Planner background and commands test gaps

- No test fails when the planner pass ignores its checkpoint offsets: with message-id dedup a pass that re-reads every source from offset 0 adds exactly the new tokens (`tokens/planner_contract_tests.rs:55`, `planner_tests.rs`'s `a_pass_reads_only_what_a_file_gained` included). A test asserting the saved offsets equal the file lengths would guard them.
- `bridge/fixture_replay.rs:115` `spool_first` relies on a fixed 300 ms sleep and degrades silently to `rest` order under load; wait on the spool's `Background` event. The negative checks after a 300 ms `settle()` in `background_contract_tests.rs:137` can pass vacuously under load (weaker mutation detection, never a false red).
- The bridge-level "no second `SubagentStopped`" check after a reopen (`tests_background_more.rs:83`) never ends the agent again; `background/tests.rs:206-213` is the real guard of the stopped set.
- The socket-to-`commandsFrameAtom` path is proven by `socket-commands.test.ts` and `commands.contracts.test.tsx`; `composer-delivery.test.tsx` writes the atom directly. Six web test files each define their own `FakeSocket`; share one.

## Planner chat web open items

- With only `/` (or a prefix such as `/comp`) typed and the list open, the foot says "Not in this session's command list" (`chat/message-problem.ts:97`), matching the server but reading as if the typed name were unknown while the list shows it; a neutral "Choose a command" would fit.
- A Terminal only entry's reason and description are separated by a space only (`command-menu.tsx:224`), so at 390px they read as one phrase; the menu lists `commandsAtom` rows directly, so after a concluded chat it can offer rows the gate refuses (pass `commandsForSend` to `useMenuEntries`); `placeholder={menu.hint ?? placeholder}` (`composer.tsx:124`) never shows the hint, since the text is non-empty whenever a hint exists.
- A failed route GET gives `unavailable` with no rows (not the built-ins), so the web refuses `/compact` and `/context` that the server accepts; transient until the next frame or hello (`state/commands-atom.ts:82`).
- `YourTurnKind` includes `background`, which is not your turn (`ui/activity-glyphs.tsx:62`); rename it. `BackgroundStatus` draws no mark for `stopped` where the list draws a muted square (`chat/tool-row.tsx:67`). `close` in `background-line.tsx:120` is recreated each render, so the Escape listener re-subscribes while the list is open.
- `chat-rise` and `.chat-empty` keep fill `both` (`chat-transcript.css:308`, `chat-surface.css:239`); a passage chip's fixed `.chat-passage` tooltip inside a user item may be offset the way the command list was (unverified), and `container-type: inline-size` on `.chat-head` (`chat-surface.css:85`) makes the head a containing block for any fixed child added later.
- `web/scripts/plan-chat-screens.mjs` repeats about 70 lines of `plan-screens.mjs` (`THEMES`, `firefoxExecutable`, `BROWSERS`, `routeSlug`, `themeInitScript`, `watchErrors`, `bootstrap`, `takeFlags`); a shared module would remove the "keep them in step" burden. The Walnut theme is retired (it maps to aubergine), so a brief naming it gets aubergine.

## Planner chat environment

- `vite build` prints the chunk-size advisory (chunks over 500 kB: `elk-*.js` 1.5 MB, `chunk-*.js` 629 kB, `plan.js` 594 kB). It predates the background and slash work (`plan.js` grew by about 16 kB); the dist criterion runs `--logLevel error`, which hides it. Code-splitting the plan bundle is open.
- `scripts/plan-journeys.sh` in a stage sandbox, on HEAD and on the plan base alike: J1 times out at its 10 s `goto` (both binaries answer nothing for about 10 s after start, the log ending "quota: codex: codex app-server closed without replying"), J2 has no sealed summaries, J3 hits the read-only `/tmp` plan lock, J4 finds no spine nav, J5 and J8 print PENDING. At the plan base J6 also fails: `/f/feat-planner/coverage` repeats labels ("Share" twice, "The model in one paragraph" seven times, "Accessibility", "Taking a passage into chat", "Permission prompts" twice). The coverage-route duplicates and the roughly 10 s startup stall are open; an operator reruns J1 to J5 and J8 outside a stage.
