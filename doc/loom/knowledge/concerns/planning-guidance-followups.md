# Planning Guidance Follow-ups
>

Open points in `planning::guidance`, `plan_server::guidance` and `plan-plugin/` after plan 8 ([architecture](../architecture/planning-guidance.md)). Each names a revisit condition.

## Unverified and unwired

- **Live sessions are unverified.** The four `#[ignore]` tests in `loom/tests/planning_guidance_operator.rs` (`operator_vision|section|metaplan|plan_session_loads_its_skill`) were not run: they need a logged-in Claude Code, tmux and network. Not shown: a real session launched through the bridge loads the rendered `loom-plan` plugin, honours the agent `model`/`effort` frontmatter and reads the session brief through `--append-system-prompt-file`. Run `cd loom && cargo test --test planning_guidance_operator -- --ignored --test-threads=1` after merge.
- **`StartBlocks::block|clear` have no production caller.** Only `Authoring` starts reach `render_session_guidance` (`plan_server/guidance/provider.rs`), so the every-kind refusal while `config-block.json` exists holds only once plans 10 and 11 call it inside their summary and pressure guidance closures. Revisit when either plan lands.
- **`reachable render_session_guidance from main` cannot pass.** The only production caller is `provider.authoring(..)` on `Arc<dyn GuidanceProvider>` (`plan_server/chat/dispatch.rs:190`); eight functions are named `authoring`, so the graph leaves the call unresolved, and no `loom stage dispute-criteria` or `amend` field covers `reachable`. Plan authors put a wiring pattern on the install call (`guidance::install`) instead of a reachable check across a trait-object seam. See [source-graph-known-gaps](source-graph-known-gaps.md).

## Behaviour limits

- Models load once at server start (`GuidanceModels::load` in `install`), so a settings-dialog edit of an authoring key applies after a restart; the spec text implies live editing.
- `PlanGuidance.rendered` grows one entry per session and is never pruned; `remember` re-reads the brief file from disk after `render_session_guidance` wrote it, where the renderer could return the text.
- Render errors carry absolute runtime paths (`plugin.rs`, `provider.rs`, `blocks.rs`) into `BridgeError::Refused`, which reaches the browser through `dispatch.rs`; the same holds for plan 7's `BridgeError::Io`. Log the chain, return `start blocked: <reason>` or a generic text.
- `GuidanceModels::load` never returns `Err` (`read_plan_config` and `UserConfig::load` are infallible), so the warn-and-default branch in `install` is unreachable; its body also copies `SummaryModels::load` line for line.
- `GET /api/session/brief` answers 404 `unknown_session` for a missing `session` parameter; 400 would be accurate.
- `plugin.rs` writes by path (safe against a final-component symlink and a fresh uuid session directory, so not session-exploitable: `sessions/` is writable by no session kind); the dirfd-anchored `safe_fs` helpers `blocks.rs` uses would also close an intermediate-directory race. `blocks.rs` reads `config-block.json` with an unbounded `serde_json::from_reader`, and no test covers its FIFO, non-regular and symlink refusals (a `mkfifo` test would catch a dropped `O_NONBLOCK`).

## Text and skills

- `common_brief.md` quotes section 03 text that points at "Section 09", "the kinds table" and "(section 01)", which in a user project resolve to the user's files or nothing; add a line listing the document paths and saying the numbers refer to loom's spec.
- `brief.rs` builds the starting-point block by parsing the rendered opening back apart; a shared (step, target, skill) struct used by `opening.rs` and `brief.rs` would end the coupling. `Target` keeps the leading verb (`Target: Write doc/product/...`) and `NoStep` renders an instruction as its target.
- `skills/retro` says "each done plan in the starting message" (the `RefineSpec` opening lists none); `skills/metaplan` says "the selected metaplan" (the `AddPlanToMetaplan` opening names none); `skills/vision` describes a first-table rule the parser (`planning/vision.rs`) does not follow; `skills/summaries` hardcodes `~/.loom/plan/<project-key>/drafts/` where the brief names the `LOOM_PLAN_STATE` directory.
- `CLAUDE.md.template`'s delegation rule pushes authoring sessions to hand document writing to subagents that lack the chat's decisions.
- `knowledge_skill_preserves_every_distill_routing_line` uses `skip(13).take(6)`, which passes for any blank line in the window; anchor it on the routing heading or assert each line is non-empty.

## Test hygiene

`user_config/plan_authoring.rs` holds inline default literals where `plan.rs` has `DEFAULT_PLAN_*` constants; `plan_config.rs` keeps an inline `authoring_tests` module beside `mod tests;`; `assets/mod.rs::plan_plugin_manifest_has_expected_name` duplicates `planning/guidance/tests.rs`'s manifest test; `plan_server/guidance/tests/install.rs::IdleHost` duplicates `chat/tests.rs::FakeHost`; four test files each carry a `LOOM_HOME` guard with no shared helper; the `config_api/tests.rs` regeneration note should name the `REGEN_FIXTURE=<path>` switch.

## Proposals for hooks and plan checks

Two mistakes recurred in the plan-8 stages, so each gets a proposal here instead of another paragraph.

- **A PreToolUse hook refusing `run_in_background` on `loom check` and `loom memory resolve`.** An integration-verify main agent ran `loom check integration-verify` in the background again; the relay never saw its `LOOM_RELAY_V1` line and the next call reported `refused: the request never reached the daemon` ([memory-relay-drain-gap](../mistakes/memory-relay-drain-gap.md)).
- **A `loom plan verify` check on gates that run only filtered targets.** `guidance-engine` passed with `--test`/vitest filters while integration-verify's full suite found a binary-spawn-guard failure, an `oxfmt --check` failure on a regenerated fixture and a stale sibling vitest ([plan-server-delivery](../mistakes/plan-server-delivery.md)). A stage that adds a `tests/*.rs` target spawning `CARGO_BIN_EXE_loom`, or regenerates `web/src/api/fixtures/config.json`, would need `--test integration` and the full web checks in its criteria.

## Operator test seam

`plan_server::chat::bridge_config` sets `BridgeConfig.loom_bin` from `current_exe()`, which in an in-process integration test is the test executable; `install_with` is `pub(crate)` and `Bridge.config` is private, so `planning_guidance_operator.rs` cannot point the hook binary at `loom` through the server's public API. Revisit when a public override for the bridge's hook binary exists.
