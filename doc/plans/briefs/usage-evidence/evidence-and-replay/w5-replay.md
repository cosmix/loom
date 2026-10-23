# W5: the signal replay harness

Tier: sonnet (`loom-software-engineer`). Read `common.md` in this directory first.

## Why and who reads it

Plans 4 (`PLAN-04-dependency-findings`) and 5 (`PLAN-05-guidance-and-gate-output`) are byte changes judged on a deterministic replay (metaplan, "How a saving is shown", class 1): the same frozen input rendered at the base and candidate revisions, diffed per section, with no provider tokens spent. Plan 5's integration-verify finds this harness by its path (`test -f tests/signal_replay.rs` from `loom/`) and runs it with `-- --ignored`, at its base from a `git archive` and at its candidate. Its file path, test name, invocation and output format below are a contract with those plans: do not rename them.

## Files you own

- `tests/signal_replay.rs` (new; the test target `signal_replay`)
- `tests/signal_replay/` (new: helper modules only)
- `tests/fixtures/signal_replay/` (new: the frozen fixture)

## The frozen fixture (`tests/fixtures/signal_replay/`)

It lives under `loom/tests/fixtures/` because `.markdownlintignore` excludes that tree: the repository's pre-commit hook runs `markdownlint-cli2 --fix` on every other tracked `.md` file and would rewrite the frozen copies.

Copy, byte for byte, from the worktree. All four sources are required; the stage's `before_stage` proves each exists. (All are committed files; copy them with the Read and Write tools or `cp`):

| Fixture path | Source | Notes |
| --- | --- | --- |
| `tests/fixtures/signal_replay/plan.md` | `../doc/plans/archive/DONE-PLAN-token-flow.md` | PLAN-token-flow: knowledge-bootstrap, token-core, tui-tokens, web-tokens, integration-verify, knowledge-distill |
| `tests/fixtures/signal_replay/knowledge/<name>.md` | `../doc/loom/knowledge/<name>.md` for `architecture`, `entry-points`, `patterns`, `conventions`, `mistakes`, `stack`, `concerns` | the tier-1 snapshot retrieval reads |
| `tests/fixtures/signal_replay/memory/token-core.md` | `../doc/plans/token-optim/briefs/dependency-findings/dependency-findings/fixtures/token-core.md` | token-core's archived journal, captured by plan 4's author |
| `tests/fixtures/signal_replay/recorded-spawn.json` | `../doc/plans/token-optim/briefs/delivery-truth/delivery-fixes/replay-payload.json` | one recorded token-core worker spawn payload, captured by plan 1's author |
| `tests/fixtures/signal_replay/requests.json` | written by you | the numbers below |

`requests.json`, measured on 2026-10-19 from PLAN-token-flow's recorded transcripts (unique assistant message ids of each stage session's main transcript; workers: the median of token-core's 14 worker transcripts, 9.5, rounded half up):

```json
{"stages": {"knowledge-bootstrap": 14, "token-core": 83, "tui-tokens": 95, "web-tokens": 45, "integration-verify": 139, "knowledge-distill": 105}, "worker": 10}
```

Add `tests/fixtures/signal_replay/README.md` listing each file, its source and the capture date. Nothing under `tests/fixtures/signal_replay/` is a symlink (`tests/maintainability/scanner.rs` refuses symlinks under `tests/`). Never edit the copies after the first commit: they are the frozen input.

## The harness (`tests/signal_replay.rs`)

Module doc, first line: `//! Replay harness: renders every stage signal of a frozen plan through format_signal_with_metrics (reached through generate_signal_with_metrics, which builds the production embedded context first) and every worker query through loom hook worker-brief, and prints bytes per section.` Declare `#[path = "integration/helpers.rs"] #[allow(dead_code)] mod helpers;` as `tests/token_optimization_contracts.rs:3-6` does; spawn the binary only through `helpers::loom_cmd()` (`tests/integration/binary_spawn_guard.rs` fails any other spawn). Put helpers in `tests/signal_replay/*.rs` included with `#[path = "signal_replay/<file>.rs"] mod <name>;` (a bare `mod x;` resolves against `tests/`; `doc/loom/knowledge/mistakes/codex-worker-briefing.md`, "Integration-Test Submodules Need `#[path]`").

One test:

```rust
#[test]
#[ignore = "replay harness: cargo test --test signal_replay -- --ignored --nocapture"]
fn signal_replay()
```

### Building the input (once per run, into a fresh `tempfile::TempDir` root R)

1. Copy `tests/fixtures/signal_replay/knowledge/*.md` to `R/doc/loom/knowledge/`, `tests/fixtures/signal_replay/plan.md` to `R/doc/plans/PLAN-token-flow.md`, and `tests/fixtures/signal_replay/memory/*.md` to `R/.loom/work/memory/`.
2. Write `R/.loom/work/config.toml`: `[plan]`, `source_path = "<R>/doc/plans/PLAN-token-flow.md"`, `plan_id = "PLAN-token-flow"`, `plan_name = "Live token flow in loom status"`, `base_branch = "main"`.
3. `let parsed = loom::plan::parser::parse_plan(&R.join("doc/plans/PLAN-token-flow.md"))?;` then for each definition `loom::models::stage::Stage::from_definition(def, &loom::models::stage::PlanIdentity::from(&parsed))`, written with `loom::verify::transitions::create_stage(&stage, &work_dir)`. If the frozen plan no longer parses at run time, fail with the parser error: never edit the fixture to make it parse. `stage.plan_id` is `PLAN-token-flow` because the file stem is the plan id; the committed fixture keeps the name `plan.md`.
4. Order the stages by dependency level, then id.

### Rendering each stage signal

For each stage in that order: skip a `StageType::Knowledge` stage (it is rendered by `generate_knowledge_signal`, a different path; print a `skipped` line, and set it `Completed` with `merged = true` as below before the next stage renders). Otherwise set its status to `Executing` with `loom::verify::transitions::update_stage(&stage.id, &work_dir, |s| { s.status = StageStatus::Executing; Ok(()) })`; build `Session::new()` with `id = format!("session-replay-{}", stage.id)`, `Worktree::new(stage.id.clone(), PathBuf::from("/replay/.worktrees").join(&stage.id), format!("loom/{}", stage.id))` (a fixed path: the dynamic section prints it, and the output must not depend on the temp root), and one `DependencyStatus { stage_id, name: stage_id, status: "Completed", outputs: vec![] }` per dependency (`orchestrator/core/stage_executor.rs:218-240` builds the same shape). Call `loom::orchestrator::signals::generate_signal_with_metrics(&session, &stage, &worktree, &deps, None, None, &work_dir)` for the `SignalMetrics`, then `generate_signal_with_skills(&session, &stage, &worktree, &deps, None, None, &work_dir, None, &[])` and take its file size (it adds the v2 section and stage feedback after the four measured sections). Then set the stage `Completed` with `merged = true` through `update_stage(&stage.id, &work_dir, |s| { s.status = StageStatus::Completed; s.merged = true; Ok(()) })`, so later stages see a merged dependency. Do not re-create a stage with `create_stage` to change it: that bails with "Stage already exists". Skill recommendations are left out (`skill_index` `None`): they depend on the host's installed skills.

### Worker queries

After the signals, for each query in this order:

1. `recorded-1`: `tests/fixtures/signal_replay/recorded-spawn.json`'s `tool_input` (stage `token-core`).
2. For each standard and integration-verify stage in stage order, each row of its description's worker table: a trimmed row of at least six `|`-separated cells `Worker | Role | Tier | Files owned | Shared context | Brief path` whose first cell matches `^[WC][0-9]+$`. The header row `| Worker | Role | ...` (three in the frozen plan) is not a worker. Skip a row whose Tier contains `codex` (Codex-bound spawns get no worker brief once plan 1 has merged). `subagent_type` = `loom-senior-software-engineer` when Tier contains `opus`, else `loom-software-engineer`; `description` = `"<Worker> <Role>"`; `prompt` = `format!("Your brief: {brief_path}. Read it in full before anything else.\n\n{row}")` with the row line as written. Query ids `w1`, `w2`, ... per stage in table order.

Run `helpers::loom_cmd()` with args `hook worker-brief`, current dir R, env `LOOM_STAGE_ID=<stage>`, `LOOM_WORK_DIR=<R>/.loom/work`, `LOOM_SESSION_ID=session-replay-<stage>`, `LOOM_HOOK_CONTEXT=1`, `HOME` and `LOOM_HOME` set to a scratch dir holding `config.toml` = `[update]\ncheck = false\n` (as `tests/token_optimization_comparison.rs:155-171` does), and stdin `{"tool_name":"Agent","tool_input":{"subagent_type":..,"prompt":..,"description":..},"session_id":"claude-replay","cwd":"<R>","transcript_path":"<R>/replay.jsonl"}`. Parse stdout as JSON: `{}` is an empty brief; otherwise take `brief` (its nonce has a fixed length, so byte counts are deterministic).

### Output (pinned format, `signal-replay v1`)

Print between the lines `=== signal-replay v1 ===` and `=== end signal-replay ===`, one record per line, fields separated by one tab, and also write the same lines to the file named by the environment variable `SIGNAL_REPLAY_OUT` when it is set:

```text
signal-replay v1 plan=PLAN-token-flow estimate=bytes-times-recorded-requests
signal <stage_id> <stage_type> stable_prefix=<n> semi_stable=<n> dynamic=<n> recitation=<n> total=<n> file=<n> requests=<n> resident_estimate=<n>
signal <stage_id> knowledge skipped=knowledge-signal
worker <stage_id> <query_id> <subagent_type> query=<n> brief=<n> envelope=<n> requests=<n> resident_estimate=<n>
asset <repo-relative path> bytes=<n>
total signal_bytes=<n> file_bytes=<n> brief_bytes=<n> asset_bytes=<n> resident_estimate=<n>
```

- `stable_prefix`, `semi_stable`, `dynamic`, `recitation`, `total` are `SignalMetrics` fields `stable_prefix_bytes` (it includes the `# Signal:` header line), `semi_stable_bytes`, `dynamic_bytes`, `recitation_bytes`, `signal_size_bytes`; `file` is the signal file size; `requests` comes from `requests.json`; `resident_estimate` = `total * requests` for a signal and `brief * requests` for a worker. It is an estimate of resident bytes replayed after admission, never a token measurement; the header line says so.
- `query` = bytes of `description + "\n" + prompt`; `brief` = bytes of the brief text (0 for `{}`); `envelope` = bytes of the stdout line without its newline.
- `asset` lines, sorted by path, relative to the repository root (`CARGO_MANIFEST_DIR/..`): `CLAUDE.md.template`, `loom-hooks/_subagent-preamble.txt`, every `agents/*.md`, every `skills/*/SKILL.md`.
- `<stage_type>` is the lowercase kebab name (`standard`, `integration-verify`, `knowledge-distill`).
- Byte counts are UTF-8 bytes. Lines appear in the order above: signals in stage order, workers in query order, assets by path, then the total.

### Self-checks the test asserts

1. Render the whole report twice, into two temp roots of different path lengths (the second nested one directory deeper), and assert the two reports are identical. A difference means a path or a clock leaked into the output: find the field and report it; never mask it.
2. Every non-skipped signal has `stable_prefix > 0` and `total == stable_prefix + semi_stable + dynamic + recitation`.
3. At least one worker delivery record under `<R>/.loom/work` (plan 1's `DeliveryRecord`, `context/delivery.rs`) has a non-empty `delivered` list; the failure message names every query and its envelope. A non-empty brief proves nothing: plan 1's `issue()` returns a brief whenever the stage declares skills, and every standard stage of the frozen plan does.
4. The report's first and last lines are the two markers; the second line is the header line; every other line splits on tab into a pinned record kind with its pinned keys; there is one `signal` line per plan stage, exactly one of them carrying `skipped=knowledge-signal`; `asset` lines are sorted and non-empty; each `total` field equals the sum over the lines above it. The `SIGNAL_REPLAY_OUT` file holds exactly the printed lines, both markers included.

Keep every file under 400 lines and every function under 50 (split into `tests/signal_replay/input.rs`, `render.rs`, `report.rs` as needed). Run once at the end: `cargo test --test signal_replay -- --ignored --nocapture` (it must finish well inside the 300-second acceptance bound; report the elapsed time), and paste the report into your final message.
