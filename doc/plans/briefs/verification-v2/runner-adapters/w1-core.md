# runner-adapters / W1 — testrun core, registry, reference adapter, captured fixtures

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D5, D6;
`doc/plans/briefs/verification-v2/fixtures/NOTES.md`; `fixtures/cargo-test/TESTS.md`.

You are wave 1. The codex units write the other 22 adapters against the trait you write, so
land the trait exactly as DESIGN D5 pins it.

## Files you own

`loom/src/lib.rs` (one `pub mod testrun;` line), `loom/src/testrun/{mod,outcome,recognize,registry,fixture_support,tests}.rs`,
`loom/src/testrun/adapters/{mod,cargo_test}.rs`, and the captured fixture directories
`loom/src/testrun/fixtures/<adapter>/` for the 14 captured adapters.

## Tasks

1. Copy `doc/plans/briefs/verification-v2/fixtures/<adapter>/` into
   `loom/src/testrun/fixtures/<adapter>/` for the 14 captured adapters, byte for byte
   (`cp -r`). Do not copy `NOTES.md`. Summarise it into a `README.md` in
   `loom/src/testrun/fixtures/`: versions, the no-match table, the ctest `build-error` pipeline
   note.
2. `testrun/mod.rs`: the public types of D5 (`RunOutput`, `RunSummary`, `RunOutcome`,
   `TestTarget`, `TestRunnerAdapter`); re-export `outcome::classify`, `registry`,
   `recognize`.
3. `outcome.rs`: `classify` exactly as D5 describes, plus unit tests for each branch.
4. `recognize.rs`: shared argv helpers every adapter uses:
   - strip runner prefixes (`env VAR=x`, `cd <dir> &&` handled by the lexer's
     `simple_commands`, `bunx`/`npx`/`pnpm exec`/`yarn`, `python`/`python3 -m`, `uv run`,
     `bundle exec`, `cargo +<toolchain>`);
   - flag helpers (`has_flag`, `flag_value`, `positionals`);
   - package-script indirection (D5): `npm test`, `npm run test`, `bun run test`, `pnpm test`,
     `yarn test` resolve through `package.json` `scripts.test` in the cwd, then the script's
     command is lexed and offered to the adapters.

   Use `crate::plan::schema::validation::shell_lex` for lexing; make it `pub(crate)` if it is
   not.
5. `registry.rs`: `all()`, `by_name()`, `recognize(command, cwd)` (D5).
   `adapters/mod.rs` defines the `adapters!` macro: each identifier `x` expands to `mod x;` plus
   a `&x::ADAPTER` entry in one `static ALL: &[&dyn TestRunnerAdapter]`. Invoke it as
   `adapters!(cargo_test);`. The main agent extends that one list after the codex units return.
6. `adapters/cargo_test.rs`: the reference implementation every codex unit imitates. It
   provides `pub static ADAPTER: CargoTest`, `name() == "cargo-test"`, `language() == "rust"`.
   - `recognizes`: `cargo test` with any prefix;
   - `is_full_run`: no positional test filter, and none of `--lib`, `--bin`, `--test`,
     `--example`, `--doc`;
   - `single_test_command`: DESIGN D5 table;
   - `select_command`: `cargo test -- <names>` over targets that carry a name, `None` when none
     does;
   - `parse`: sum every `running N tests` and every `test result: ... N passed; M failed; K ignored`
     line across test binaries; `error[E` or `could not compile` ⇒ `build_failed`.

   Keep the parse helpers small and reusable (`sum_matches(regex, text)`), in `recognize.rs` or
   a small `parse_util` section, so codex units can call them.
7. `fixture_support.rs` (`#[cfg(test)]`): `load(adapter, scenario) -> (String, String, Option<i32>)`
   reads `<scenario>.stdout`, `.stderr` and the `exit:` line of `.meta` at test time from
   `concat!(env!("CARGO_MANIFEST_DIR"), "/src/testrun/fixtures/")`, and
   `scenarios(adapter) -> Vec<String>` lists the scenario names present. No new dependency.

## Named tests (binding), in `testrun/tests.rs` (declared from `mod.rs`)

- `registry_lists_every_adapter`: `registry::all()` names equal exactly the 23 names of D5 (set
  equality). This fails until the main agent registers every codex adapter. That is expected
  during wave 1.
- `every_fixture_classifies_as_expected`: for every registered adapter, for every scenario
  directory present under its fixtures (`one-pass`, `one-fail`, `no-match`, `suite`,
  `build-error`), `classify(parse(fixture), exit)` gives the D5 expectation. `suite` gives
  `executed == Some(3)` and `failed == Some(1)`. Variant files `<scenario>.<variant>.*`
  (`.k` for pytest, `.nocolor` for several runners) carry their base scenario's expectation and
  are asserted too.
  An adapter missing any of the four non-build scenarios fails the test.
- `recognizes_prefixed_invocations`: `cd loom && cargo test --manifest-path loom/Cargo.toml x`,
  `env RUST_LOG=1 cargo test`, `cargo +nightly test` → `cargo-test`; `echo cargo test` → none.
- `full_run_detection_ignores_filtered_runs`: `cargo test --all-targets` is a full run;
  `cargo test --lib x::` and `cargo test foo` are not.
- `package_script_indirection_is_recognized`: a temp dir whose `package.json` has
  `"scripts": {"test": "cargo test"}` makes `npm test` recognised as `cargo-test`. (Use
  cargo-test because it is the adapter registered in wave 1.)

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib testrun::`
(`registry_lists_every_adapter` is expected to fail in wave 1; every other test passes.)

## Report

Files created; the exact public API as written (paste the trait and the helper signatures, so
the main agent can check the codex briefs against it); the proof result.
