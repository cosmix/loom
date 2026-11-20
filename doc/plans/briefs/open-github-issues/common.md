# Rules for Every Worker of PLAN-open-github-issues

Read this first, then your own brief. The plan is `doc/plans/PLAN-open-github-issues.md`; its YAML is authoritative where prose differs.

## Boundaries

- Write only the files your brief owns. Every other file is read-only, including the frozen contract files your stage signal lists (`loom stage contracts show` output is pasted into your brief by the orchestrator when relevant). A frozen contract that you believe is wrong is reported to the orchestrator, never edited.
- Never run `git`. Never read or write anything under `.loom/`.
- Never edit `loom/maintainability-baseline.txt`. A Rust or TypeScript file stays at or under 400 lines (or at or under its ledger row if the ledger lists it), a function at or under 50 lines. To grow a ledgered file, move lines out first (a new sibling module).
- No placeholders, stubs, `todo!()`, `unimplemented!()` or empty functions. No backward-compatibility or migration code: the project is unreleased.
- No mention of any AI system in code, comments or test names.

## Rust (`loom/`)

- Errors: `anyhow::Result` with `.context(..)` at I/O boundaries; no `unwrap()` or `expect()` outside tests.
- New modules are declared with a plain `mod` line (no `#[path]`); test modules are `#[cfg(test)] mod <name>_tests;` beside the code.
- Format with `cargo fmt --manifest-path loom/Cargo.toml` and lint with `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings`.
- Literal braces inside `format!` are doubled; a CLI placeholder in a rustdoc comment goes in backticks.
- Tests that need the user config directory use `crate::user_config::redirect::redirect_user_config`; never the real home.
- Prove your slice with the module filter your brief names: `cargo test --manifest-path loom/Cargo.toml --lib <module>::`. Never run the whole suite.

## Web (`web/`)

- TypeScript strict; no parameter properties (`erasableSyntaxOnly`); zod schemas stay `.strict()`.
- UI chrome is the body face, sentence case, 12-13 px weight 500; monospace only for identifiers and values; colours only through the theme tokens in `web/src/index.css` and `web/src/aurora-ui/shared/styles/tokens-*.css` (knowledge: `conventions/web-dashboard-typography.md`). Models read `model/effort` through `modelWithEffort` (`web/src/lib/format.ts`).
- Run tests from `web/`: `./node_modules/.bin/vitest run <paths>`; type check `./node_modules/.bin/tsc -b --noEmit`; lint `./node_modules/.bin/oxlint --deny-warnings`. The orchestrator runs `oxfmt` after you return.

## Report

Return: files changed, the proof command and its result, anything surprising, anything unresolved. Record a mistake or a surprising fact with `loom memory note "..."` immediately.
