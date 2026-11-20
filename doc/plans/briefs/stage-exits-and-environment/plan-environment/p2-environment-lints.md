# P2: plan-time environment lints

Stage `plan-environment`, wave 2, tier sonnet. Read `../common.md` first. P1 has added
`loom::plan::schema::ProvisionEntry { working_dir, command }` and
`LoomConfig.provision: Vec<ProvisionEntry>` (a `version: 2` field); read them before you start.

You own `loom/src/plan/schema/validation/v2_lints/{mod.rs, registry_domains.rs (new),
repo_hooks.rs (new), js_provision.rs (new), sandbox_capability.rs}`,
`loom/src/plan/schema/tests/{mod.rs, v2_lint_environment_tests.rs (new)}`,
`loom/src/commands/plan/verify.rs`, `loom/tests/fixtures/plans/v2-environment-lints.md`
(new) and the header paragraph of `loom/tests/fixtures/plans/v2-valid.md`.

## Why

Every stage of PLAN-source-graph-mechanism ran the pre-commit hook's `bunx markdownlint-cli2`
with no npm registry in its sandbox, and completion ran `bunx vitest` in a worktree with no
`node_modules`. No lint knew which registry `bunx` reaches, nothing read the repository's git
hooks, and nothing asked how a JS package gets its dependencies. `loom plan verify` should
catch all three before `loom run`.

## How lints work here

`v2_lints/mod.rs::run` calls each lint's `check(ctx: &LintContext, out: &mut
Vec<LintFinding>)`; a finding is an error on a v2 plan when `error_in_v2`, a warning
otherwise (`split_lint_findings`), and `--strict` fails on either. `LintContext { metadata,
repo_root: Option<&Path> }`: lints read the repository and write nothing. `visit_argvs(script,
0, &mut |argv| ..)` gives the argv of every simple command, nested `$(..)` and `sh -c` scripts
included, with assignments and `env`/`command`/`exec`/`timeout` wrappers skipped. Read
`mod.rs`, `sandbox_capability.rs` and `rust_filters.rs` (which returns notes) first.

## 0. Give the lints an absolute repository root

`commands/plan/verify.rs::find_repo_root` (about line 106) walks the ancestors of the plan
path's parent. For a relative path (`loom plan verify doc/plans/X.md`) the last ancestor is
`""`, `"".join(".git")` exists at the checkout root, and `repo_root` becomes `Some("")`; `git`
then runs with `current_dir("")` and fails. That is why `loom plan verify doc/plans/X.md` notes
"the repository has no HEAD commit" today. `verify.rs` is ledgered at 555 lines: change the one
call line (about line 386) to

    let repo_root = find_repo_root(&std::path::absolute(path)?);

and nothing else in the file.

Consequence to record: with a real root, the repository-level lints (the pre-commit hook and
the JS provision lint) judge the checkout the plan sits in. `tests/fixtures/plans/v2-valid.md`
has no sandbox block and no `provision`, so `loom plan verify --strict` of it INSIDE this
checkout now fails by design (the hook's `bunx`, and `web/` with no `provision`), although its
header and `CONTRIBUTING.md` ("A plan-version-2 change ... is exercised by
`loom/tests/fixtures/plans/v2-valid.md` under `loom plan verify --strict`") say it must pass.
Rewrite the fixture's header sentence to: "`loom plan verify --strict` must accept it in a
repository with no JS package and no registry-fetching git hook (a scratch git repository
holding only this file); inside the loom checkout the repository-level lints fire by design."
Do not change its YAML: the contracts copy it into scratch repositories. Knowledge-distill
rewrites the `CONTRIBUTING.md` line.

## 1. Registry domains: `registry_domains.rs`

`pub(super) fn registry_need(argv: &[&Word]) -> Option<(String, &'static [&'static str])>`
returns the tool label and the domains it fetches from, after skipping `xargs` and its
options (`-0`, `-r`, `-n N`, `-I X`, `-P N`, `-d X`, `-L N`; any word starting `-`, plus the
value of `-n`, `-I`, `-P`, `-d`, `-L`, `-a`, `-E`, `-s`) so `xargs -0 bunx ...` reads as `bunx
...`:

| Command | Label | Domains |
| --- | --- | --- |
| `bunx`, `npx`, `bun x`, `pnpm dlx`, `yarn dlx` | the launcher | `registry.npmjs.org` |
| `npm`, `bun`, `pnpm`, `yarn` with `install`, `i`, `ci` or `add` | e.g. `bun install` | `registry.npmjs.org` |
| `cargo install`, `cargo fetch` | as written | `crates.io`, `index.crates.io`, `static.crates.io` |
| `uv sync`, `uv add`, `uv pip install`, `uvx`, `pip install`, `pip3 install`, `python -m pip install`, `python3 -m pip install` | as written | `pypi.org`, `files.pythonhosted.org` |
| `go get`, `go mod download` | as written | `proxy.golang.org` |

`pub(super) fn allows(pattern: &str, host: &str) -> bool`: ASCII case-insensitive; `*`
allows every host; `*.suffix` allows a host that ends with `.suffix` (not `suffix` itself);
anything else allows only itself. These are the patterns `plan/schema/validation.rs::
validate_domain_pattern` accepts.

`pub(super) fn check(ctx, out)`: for every stage whose effective sandbox is enabled
(`stage.sandbox.enabled.unwrap_or(plan.sandbox.enabled)`, as `sandbox/config.rs::merge_config`
resolves it), the effective network is the stage's `sandbox.network` if set, else the plan's
(merge_config replaces it whole); the allowed set is `allowed_domains` plus
`additional_domains`. Scan the stage's acceptance, setup, wiring-test, after-stage and
dead-code commands. Skip `before_stage`: it runs on the daemon's host, not in the sandbox. Add
`fn sandboxed_commands(stage)` to `mod.rs` beside `stage_commands` for that list. For each
registry need with a domain no allowed pattern covers, push an error-in-v2 finding:

    <label> `<command>` runs `<tool>`, which needs <needed>; the stage's sandbox does not allow <missing>: add it to `sandbox.network.allowed_domains` or `additional_domains`, or install from a plan `provision` entry

(`<label>` and `<command>` via `StageCommand::describe`; `<needed>` and `<missing>` joined with
", ".) One finding per command and tool.

`sandbox_capability.rs` keeps its "no domain at all" error for `curl`, `wget`, `gh` and `cargo
audit`, and skips the `npm install`, `bun install` and `cargo install` hazards, which the
registry lint now reports with the domains they need (otherwise a no-domain stage gets two
errors for one command). Keep `criterion_hazards.rs` unchanged: its network warning list is a
separate check.

## 2. The repository's pre-commit hook: `repo_hooks.rs`

`pub(super) fn check(ctx, out)`, only when `ctx.repo_root` is `Some`:

1. Find the hooks directory with SCOPED reads, the way
   `orchestrator/core/merge_handler/merge_gate.rs::hooks_dir_prefix` does: the first of
   `run_git_checked(&["config", scope, "--get", "core.hooksPath"], root)` for `--local`, then
   `--global`, then `--system`, that returns a non-empty value (an error means unset at that
   scope). A relative value is joined to `root`. When it is unset at every scope, use
   `run_git_checked(&["rev-parse", "--git-common-dir"], root)` joined with `hooks` (joined to
   `root` when relative). Never use `rev-parse --git-path hooks` or an unscoped
   `config --get`: loom's git runner prepends `-c core.hooksPath=/dev/null` to every command
   (`git/runner.rs::NO_HOOKS_ARGS`), which wins on precedence, so both always answer
   `/dev/null` and the lint would silently find no hook
   (`mistakes/sandbox-tooling-and-network.md`, "`-c core.hooksPath=/dev/null` Silently
   Overrides a Scoped `core.hooksPath` Read"). In this checkout the local scope answers
   `loom/.githooks` (set by hand per `CONTRIBUTING.md`; a clone without it has no hook to
   lint). Any other git failure means no finding.
2. Read `<hooks>/pre-commit` with `crate::fs::safe_read::read_to_string_bounded(&hooks_dir,
   Path::new("pre-commit"), 1 << 20)`; absent or unreadable means no finding.
3. `visit_argvs` over the whole script and `registry_need` on each argv.
4. For every stage with an enabled sandbox (every stage commits, so the hook runs in each),
   and every need with a domain that stage does not allow, push an error-in-v2 finding on
   that stage:

       the repository's pre-commit hook `<hook path relative to root>` runs `<tool>`, which needs <needed>; the stage's sandbox does not allow <missing>, so the hook cannot do its work when the stage commits: allow the domain or change the hook

One finding per stage and tool.

This repository's own hook (`loom/.githooks/pre-commit`) has the line
`MD_OUT=$(git ls-files -z -- '*.md' | xargs -0 bunx markdownlint-cli2 --fix 2>&1) || true`;
your tests must find `bunx` in that shape.

## 3. JS packages need a provision entry: `js_provision.rs`

`pub(super) fn check(ctx, out) -> Vec<String>` (notes, like `rust_filters::check`), only for
`version: 2` plans (`provision` is v2-only) and only when `ctx.repo_root` is `Some`:

- `crate::skills::project::ProjectProfile::discover(root)` and `package_details()`. A package
  whose `runner` names an adapter with `language() == "javascript"`
  (`crate::testrun::registry::by_name`) needs a provision entry whose `working_dir`, as a
  path, equals the package path or is an ancestor of it (`.` covers every package; P1 rejects
  an empty `working_dir`). Fixture directories are not scanned (stage `completion-gates` adds that to
  detection); do not filter them here.
- Each uncovered package is a plan-level error-in-v2 finding (`stage_id: None`):

      package `<path>` runs its tests with <runner> and no `provision` entry covers it: impact-selected tests and integration-verify cannot run it in a worktree, which has no node_modules; add `{ working_dir: "<path>", command: "<install>" }` to `loom.provision`

  `<install>` follows the package's lockfile: `bun.lock` or `bun.lockb` → `bun install
  --frozen-lockfile`; `pnpm-lock.yaml` → `pnpm install --frozen-lockfile`; `yarn.lock` →
  `yarn install --frozen-lockfile`; `package-lock.json` → `npm ci`; none → `npm install`.
- Each provision entry whose `root.join(working_dir)` is not a directory is an error-in-v2
  finding: "provision entry #<n> working_dir `<dir>` does not exist in the repository".
- A truncated scan (`profile.truncated`) returns the note "the package scan stopped at its
  depth or entry limit, so a JS package may be missing from the provision check". In this
  checkout the scan is always truncated (the depth limit, `skills/project/scan.rs`), so this
  note appears on every verify here.
- The lint judges the repository, not the stages: a v2 plan in this repository that runs no
  JS still needs a `provision` entry for `web`. That is the settled design (plan Choice 20).

Register all three in `run`: `registry_domains::check(ctx, &mut out);`,
`repo_hooks::check(ctx, &mut out);`, `notes.extend(js_provision::check(ctx, &mut out));`.

## 4. Tests

`plan/schema/tests/v2_lint_environment_tests.rs`, registered in `plan/schema/tests/mod.rs`.
Use the helpers other lint test files use (read `v2_lint_repo_tests.rs` and
`v2_lint_command_tests.rs`, and `tests/mod.rs`'s `create_valid_metadata_v2`). Cover: each row
of the registry table and a near-miss (`npm test`, `cargo build`, `go test`), `xargs -0 bunx`,
a wildcard and a bare `*`, a stage override that replaces the plan network, a disabled
sandbox, `before_stage` skipped; the hook lint with a relative `core.hooksPath` set in a TempDir repo,
with an absolute one, without it (a `.git/hooks/pre-commit`), and with no hook, each through
`check` itself (the production lookup, never a test-only path helper); the JS lint covered by an exact entry,
by an ancestor entry, uncovered, a missing entry directory, and a v1 plan (no finding).

## 5. The fixture plan

`loom/tests/fixtures/plans/v2-environment-lints.md`: a `version: 2` plan in the shape of
`tests/fixtures/plans/v2-valid.md` with a `summary:` line added to every stage (v2-valid has
none; `plan verify` warns on each), with plan-level
`sandbox: { network: { allowed_domains: ["crates.io"] } }` and `bunx tsc --noEmit` added to the
standard stage's acceptance. Verified from this repository (`web/` runs vitest; the pre-commit
hook runs `bunx`), it must report all three lints: the stage's own wiring test runs
`cargo run --quiet -- plan verify --json tests/fixtures/plans/v2-environment-lints.md` from
`loom/` and expects exit 1 with `registry.npmjs.org`, `pre-commit hook` and `package`web``
in stdout (one needle per lint: the registry lint's own message also says "provision", so
"provision" alone would not prove the JS lint runs). A header paragraph says what the fixture
is for, and that its hook finding depends on this checkout's local
`core.hooksPath=loom/.githooks` (a clone without that setting reports no hook).

## Contracts your code must satisfy

`js-package-without-provision-is-an-error`, `registry-tool-without-registry-domain-is-an-error`,
`pre-commit-hook-registry-need-is-an-error` (scenarios in the plan's YAML).

## Check

`cargo test --lib plan::schema::tests::v2_lint_environment_tests`, once. The main agent also
runs `cargo test --test integration plan_verify` (26 tests of the `loom plan verify` binary).
