# Contributing to Loom

Thanks for your interest in contributing to Loom.

Loom is pre-1.0 and under active development: do not add backwards-compatibility shims or migration routines; change the code and its callers directly.

## Code of Conduct

- Be respectful and constructive in all interactions
- No harassment, discrimination, or hateful behavior
- Assume good intent; ask clarifying questions before criticizing
- Keep discussions focused on the work, not the person

Violations will result in removal from the project.

## The AI Rule

You may use AI/Agentic development tools to help write code. However:

**You are responsible for every line you submit.**

"AI slop" — code that's clearly been generated without understanding, review, or testing — will get your PR summarily rejected and repeated offenses will get you banned from this repository.

What this means in practice:

- Work on a small, focused surface area
- Understand what your code does
- Test it thoroughly
- Review it as if someone else wrote it
- Don't submit boilerplate-heavy, over-engineered, or obviously templated code

## Before Contributing

1. **Check existing issues** — Your idea may already be discussed
2. **Open an issue first** for significant changes — Get buy-in before investing time
3. **Small PRs are better** — Easier to review, faster to merge

## Repository Layout

| Path | Contents |
| --- | --- |
| `loom/` | The Rust crate (CLI, daemon, orchestrator). Run every `cargo` command from here. |
| `loom/tests/` | Integration tests (`tests/integration/`, shared helpers in `tests/integration/helpers.rs`), e2e tests, and the maintainability gate. |
| `loom/.githooks/` | The repo's `pre-commit` and `pre-push` hooks. |
| `loom-hooks/` | The Claude Code hook scripts loom installs; embedded into the binary at build time. |
| `web/` | The `loom status --web` dashboard (React + Vite, built with bun). The built `web/dist` is committed and embedded into the binary at build time. |
| `agents/`, `skills/`, `commands/`, `codex/` | Agent assets that `install.sh` installs into `~/.claude/` and `~/.codex/`. |
| `CLAUDE.md.template`, `AGENTS.md.template` | The orchestration rules installed as `~/.claude/CLAUDE.md` (and the Codex equivalent). Edit the template, never an installed copy. |
| `doc/loom/knowledge/` | Curated project knowledge; start at `INDEX.md`. |
| `doc/plans/` | Loom execution plans. |
| `scripts/` | CI and developer scripts (`flake-check.sh`, `guarded-cargo.sh`, `check-hook-syntax.sh`, smoke tests). |

## Development Setup

Prerequisites:

- Rust stable via `rustup`. CI installs the latest stable, so a new clippy lint can fail CI while passing locally; run `rustup update stable` regularly (the pre-push hook warns when a newer stable exists).
- `git`, `jq` (every hook parses its payload with it — required), `rg` and `fd` (recommended).
- `bun` — both git hooks lint markdown with `bunx markdownlint-cli2`, and the web dashboard builds with it.
- `cargo-audit` (`cargo install cargo-audit`) — the pre-push hook runs `cargo audit`.
- `tmux` — optional; used by the tmux-backend e2e tests. CI's Linux test job installs it and sets `LOOM_E2E_REQUIRE_TMUX=1`.

Enable the git hooks once, from the repo root:

```bash
git config core.hooksPath loom/.githooks
```

Build: `cd loom && cargo build`. To install your build as the `loom` on your `PATH` together with its hooks, agents and skills, run `bash ./dev-install.sh` from the repo root (it builds the release binary and runs `install.sh`). Use it instead of copying the binary by hand, so the installed hooks and assets match the binary.

Web dashboard: `cd web && bun install && bun run build`. Without `web/dist`, the build warns and `loom status --web` answers 503.

## The Gate: What Must Pass Before You Push

The authoritative list is `loom/.githooks/pre-push`; CI runs the same checks. Read that file rather than assembling a subset from memory. From `loom/`:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo audit
cargo test --all-targets --no-fail-fast
../scripts/flake-check.sh
```

Plus markdown lint from the repo root (the hook lints tracked `.md` files except `doc/plans/` and `loom/tests/fixtures/`):

```bash
git ls-files '*.md' | grep -v '^doc/plans/' | grep -v '^loom/tests/fixtures/' | xargs bunx markdownlint-cli2
```

and `cd web && bun run check` (typecheck, oxlint, oxfmt check, vitest) when you touch `web/`.

Why the flags matter:

- `cargo clippy` without `--all-targets` checks only the library and binary, skipping `#[cfg(test)]` modules and `loom/tests/`; CI lints them.
- Plain `cargo test` stops at the first failing target and hides the rest; a green final `test result: ok` line does not mean the suite passed. A line `error: test failed, to rerun pass --test <name>` means the run was incomplete — rerun with `--no-fail-fast`.
- rustdoc lints are invisible to build, clippy and test. In a doc comment, `` [`name`] `` must resolve to a public item; for a private `fn`, `const` or field write a plain code span `` `name` ``.
- `scripts/flake-check.sh` reruns the timing-sensitive test modules repeatedly under CPU load (pinned to 4 CPUs where `taskset` exists) to flush races that a single run passes.

What the hooks do:

- `pre-commit`: refuses a commit when a staged file also has unstaged changes (stage whole files); runs `cargo fmt` and `markdownlint-cli2 --fix`, then re-stages the files you staged; runs the maintainability gate (`cargo test --quiet --test maintainability`); runs the rustdoc gate when the staged diff adds or edits a `///` or `//!` line.
- `pre-push`: the gate above. If markdown lint auto-fixes anything, the push stops so you can commit the fixes.

CI (`.github/workflows/ci.yml`) runs when a push to `main` or a pull request touches `loom/`, `web/` or `scripts/`: build, tests (Linux full run; macOS compile-only), clippy, maintainability, flake-check, docs, fmt, hook syntax (`scripts/check-hook-syntax.sh`, `scripts/test-pre-commit-partial-staging.sh`), `cargo audit`, `cargo deny check`, the web job (`bun install --frozen-lockfile`, `bun run check`, and a check that the committed `web/dist` matches a fresh build), and smoke tests of the web dashboard and terminal.

## Code Guidelines

- Size limits: file 400 lines, function 50 lines, impl block 300 lines. `cargo test --test maintainability` enforces file and function limits, measured after rustfmt. Legacy exceptions live in `loom/maintainability-baseline.txt`, an exact-match ledger: it fails when a listed item grows AND when it shrinks. Never add or raise an entry; extract into a new module instead. If a refactor brings a listed item under the limit, delete its entry. Before growing a function, `grep` for it in the baseline.
- Splitting a file: use `<name>.rs` plus a `<name>/` directory; do not convert an existing `<name>.rs` into `<name>/mod.rs`.
- Errors: application code returns `anyhow::Result` with context at layer boundaries; use a typed error only when callers branch on the variant; git errors include the command, directory, exit code, stdout and stderr.
- Comments: sparing, only for non-obvious reasons. Doc comments describe the current wiring, not the intended one.
- Dependencies: add with `cargo add` / `bun add`; never hand-edit a manifest.
- Version: the product version is the git tag. `loom/Cargo.toml` carries the placeholder `0.0.0-dev`, so `env!("CARGO_PKG_VERSION")` is `0.0.0-dev` in every build; use `crate::version::VERSION`. Never bump the Cargo.toml version.
- Follow the patterns of the surrounding code; no drive-by refactors or reformatting in unrelated files.

## Writing Tests

- Name tests `test_<action>_<condition>`; isolate filesystem state with `tempfile::TempDir`; mark tests that mutate process-global state (env vars, working directory, `PATH`) `#[serial]` — and a test that reads a variable a `#[serial]` test mutates must be `#[serial]` too.
- Integration tests spawn the loom binary only through `loom/tests/integration/helpers.rs` (`loom_cmd`), enforced by `tests/integration/binary_spawn_guard.rs`. The helper clears inherited `LOOM_*` session variables and points `LOOM_HOME` at a scratch directory.
- Never touch real state: no writes to the real `~/.loom`, `~/.claude`, or the repo's own `.loom/work/`. Pass explicit temp paths (`LOOM_HOME`, `LOOM_WORK_DIR`) instead of relying on upward directory discovery.
- Never create a process that can outlive the test. `cargo test` reports nothing about a leaked detached child; one such leak once exhausted 125 GB of RAM. Guard the spawn at the lowest level, and wrap any `Child` a test holds in a guard that kills and waits on drop.
- Tests that shell out to `git` must assert every setup command's exit status and isolate ambient git config (`GIT_CONFIG_GLOBAL` / `GIT_CONFIG_SYSTEM` pointed at nonexistent paths, `GIT_CONFIG_NOSYSTEM=1`).
- CI runners have no terminal emulator; a test that builds an `Orchestrator` must configure the tmux backend so no terminal detection runs.
- Build identity: every untagged commit builds a `-dev` prerelease; a commit a release tag points at builds the bare release version, so the suite also runs against a release build (the pre-push hook of a tag push, and the release workflow). A test whose expectation depends on this reads `loom::version::VERSION` instead of assuming `-dev`.
- A test must be able to fail: drive the real entry point, assert on data production code produced, and when you cap something assert both the ceiling and a floor. Prove a fix by mutation — remove the fix, watch the test go red, restore it, watch it go green.
- Don't make a success-path deadline tighter than production's; timing-sensitive tests are what `scripts/flake-check.sh` exists for.
- Optional: `scripts/guarded-cargo.sh` runs a command in its own process group under a RAM watchdog and reports any `loom` process still alive afterwards, e.g. from `loom/`: `MIN_AVAIL_GB=8 ../scripts/guarded-cargo.sh cargo test --all-targets --no-fail-fast`. Its default floor is 32 GB of available memory, so lower `MIN_AVAIL_GB` on smaller machines.

## Hook Scripts and Agent Assets

- Hook scripts in `loom-hooks/` are bash, must work on Linux and macOS (POSIX `awk`, no GNU-only extensions), and parse their payload with `jq`. Blocking guards call `loom_require_jq` from `loom-hooks/_common.sh` (fails closed); advisory hooks call `loom_warn_no_jq`. Run `scripts/check-hook-syntax.sh` after editing one.
- Hooks, agents, skills and the templates are embedded into the binary at build time: to try an edit in a real session, rebuild and run `dev-install.sh`.
- Some guidance blocks must appear byte-identically on several surfaces (`CLAUDE.md.template`, generated signal files, hook messages). Equality tests in `loom/src/orchestrator/signals/tests_doctrine*.rs` pin them; change every surface together.

## Project Knowledge

- `doc/loom/knowledge/` holds curated knowledge: tier-1 summaries (`architecture.md`, `conventions.md`, `mistakes.md`, ...) and tier-2 topics under `<category>/<slug>.md`. Read `INDEX.md` first, then only the sections it points to. `loom knowledge context --query "<question>"` returns matching sections; `loom map --outline <file>`, `loom map --find-all <symbol>` and `loom map --impact <symbol|path>` query the source graph.
- `mistakes/` records defects that already happened in this codebase; read the entries for a subsystem before changing it.
- Record what you learn in the same change: a mistake gets a `## <Short description>` heading with **What happened**, **Why**, **Prevention**, **Fix**. `loom knowledge update <category>/<slug>` appends to (or creates) a topic and regenerates `INDEX.md`; `loom knowledge replace-section <file> "<Heading>" "<body>"` corrects a section in place (`update` only appends). Never hand-edit `INDEX.md`.
- Knowledge is reference data. When it contradicts the code, the code wins — fix the knowledge in your change.

## Submitting Changes

### Commits

- Conventional Commits: `type(scope): description`, with type `feat`, `fix`, `refactor`, `test`, `docs` or `chore` — e.g. `fix(skills): discover project types for skill routing`.
- Group by logical purpose; keep each fix or feature with its tests; separate unrelated changes.
- Stage whole files (the pre-commit hook refuses partially staged files).
- No AI attribution in commit subjects, bodies or trailers (no generated-by lines, no AI co-author trailers).

### Pull Requests

1. Fork the repository and create a feature branch
2. Ensure the gate above passes
3. Write tests for new functionality
4. Update documentation if needed
5. Update or add knowledge entries when the change teaches something non-obvious
6. Open a PR with a clear description of what and why

### What Makes a Good PR

- Solves one problem well
- Includes tests
- Doesn't break existing functionality
- Code is readable and follows existing patterns
- No unnecessary changes (formatting wars, refactors unrelated to the PR)

### Releases

Maintainers cut a release by pushing a `v*.*.*` tag. `.github/workflows/release.yml` builds from the tag, runs the tests, and fails unless the binary's `loom -v` version equals the tag without its leading `v`.

## Questions?

Open an issue. We're happy to help.
