# W4: Main-Agent Full-Suite Guard, Every Runner

Tier: sonnet. Files owned: `../loom-hooks/subagent-verify-guard.sh`, `tests/integration/hooks_subagent_verify_guard*.rs` (existing, plus a new `hooks_subagent_verify_guard_main.rs`), and `tests/integration/mod.rs` (declare the new module only). Read `common.md` first.

## Current state (`1b5a0c91`, the hook is 428 lines)

- `loom_is_subagent "$INPUT_JSON" || exit 0` at :66 lets the main agent through everything.
- Carve-outs: a `loom-verifier` subagent (:68-79), and any subagent of an integration-verify stage (:81 onward, an exact single `stages/*-${LOOM_STAGE_ID}.md` match).
- Classifiers:
  - `check_cargo` (:275): `build`, `clippy`, `fmt` and `check` always block; `test` and `nextest run` go through `block_unless_scoped` (:301), which uses `scan_args` (:213).
  - `check_tsc` (:309), `check_eslint` (:317), `check_go` (:327), `check_node_pm` (:335), `check_make` (:350).
  - The dispatcher is `check_command` (:355), on cargo, pytest, tsc, eslint, go, the node package managers and make.
  - The token walk skips prefixes (`env`, `sudo`, `bunx`, ...). Direct `vitest`, `jest`, `mocha`, `rspec`, `phpunit`, `dotnet test`, `mvn test`, `gradle test`, `mix test`, `swift test`, `dart test`, `flutter test`, `ctest` and `node --test` are not classified.
- The hook ships inside the binary: `loom::fs::permissions::constants::HOOK_SUBAGENT_VERIFY_GUARD`. Do not add a new sourced file, which would need asset registration outside your territory.
- Tests embed the hook through that constant and simulate process trees. Read GOTCHA 1 and GOTCHA 2 at the top of `tests/integration/hooks_subagent_verify_guard.rs`.

## Steps

1. **Main agent.**
   - Replace the early exit at :66. A main agent proceeds to classification only when a live loom STANDARD stage session is detected: the same live-session precondition `loom_is_subagent` gates on, plus the stage file declaring `stage_type: standard`. Reuse the integration-verify file lookup and refuse on zero or multiple matches the same way.
   - For the main agent, block only unfiltered TEST-SUITE runs. Build, clippy, fmt, tsc, eslint and make remain allowed for the main agent, since lint and format are cheap and the review order needs formatting.
   - The refusal for the main agent says: "the stage gate runs once, in loom check <stage-id>; re-check one criterion with loom check <stage-id> --criterion <n>; run your own work with a filtered test".
   - Subagent behaviour is unchanged.
2. **Every runner.**
   - Add classifiers for the runners listed above. Each one is unfiltered unless the command has a positional path or test name, or the runner's own name, tag or filter option:
     - vitest/jest `-t`, `--testNamePattern`;
     - mocha `--grep`;
     - rspec `-e`, `--tag`;
     - phpunit `--filter`;
     - dotnet `--filter`;
     - mvn `-Dtest`;
     - gradle `--tests`;
     - go `-run`, or a package other than `./...`;
     - pytest `-k`, or a path;
     - ctest `-R`;
     - swift `--filter`;
     - dart and flutter `--name`, or a path;
     - mix a path.

     Mirror the adapters' own `is_full_run` choices in `loom/src/testrun/adapters/*.rs`; read each one's range.
   - Recognise them after the existing prefix skipping, including `bunx`, `npx`, `pnpm exec` and `uv run`.
   - Keep the file's style. If the file passes 500 lines, compact the classifiers into one table-driven case statement rather than splitting the file.
3. **Tests.**
   - New `hooks_subagent_verify_guard_main.rs`:
     - main agent of a standard stage blocked on `cargo test`, `bunx vitest run`, `uv run pytest` and `go test ./...`;
     - main agent allowed on `cargo test --lib a::`, `bunx vitest run src/a.test.ts`, `cargo clippy --all-targets` and `cargo fmt --check`;
     - main agent of an integration-verify stage allowed on `cargo test`;
     - no live loom session allowed on everything.
   - Extend `_cases.rs` with one blocked and one allowed case per new runner, for a subagent.
   - Then run `cargo test --test integration hooks_subagent_verify_guard`. Run it with `TMPDIR` set to a directory without "claude" in its path (GOTCHA 1); the sandbox's default `TMPDIR` breaks these tests, which is not your defect.

## Contract you must turn green

`guard-blocks-main-agent-full-vitest-run`.

## Proof

`cargo test --test integration hooks_subagent_verify_guard`, `cargo test --test gate_dedup_contracts guard`, `bash -n ../loom-hooks/subagent-verify-guard.sh`.
