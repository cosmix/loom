# Plan: Planning session sandbox

Implements: section_12

## Overview

Every Claude Code session `loom plan` starts (authoring, summary, pressure) runs under a settings
file loom writes for it and a Rust path guard Claude Code calls before every file-tool use. This
plan builds both, testable before any session exists: `loom::plan_session::sandbox` writes section
12's three-layer settings file per session kind (OS sandbox lists, permission rules, hook
registration), and `loom plan hook PreToolUse --session-dir <dir>` decides each Read, Edit, Write,
MultiEdit, NotebookEdit, Grep and Glob call against the live tree with plan 1's shared path
predicate, refusing symlinks and failing closed. A confinement test runs loom's own document
commands inside a real sandbox built from the settings. Plan 7 (the session bridge) writes these
files at launch; plan 11 selects the pressure kind.

Product spec: `doc/product/section_12.md` ("Planning sessions" through "Sandbox prerequisites",
"Paths in requests", "Symlinks under `doc/`"), with the hook command form of
`doc/product/section_07.md` ("Hooks and the event spool"). Metaplan entry 6 of
`doc/plans/metaplan-planner-ui.md`. The public API is in
`doc/plans/briefs/feat-planner-06-session-sandbox/common.md`.

## Goals

- One builder, `plan_session::sandbox::build_settings`, produces the settings for all three kinds;
  every file it writes passes `assert_planning_settings` (no bypass mode, sandbox on and fail-closed,
  no unsandboxed escape, no network).
- The write table of section 12 holds in all three layers: the OS sandbox lists for Bash, `Edit(//...)`
  rules for file tools, and the guard as the precise layer that sees a plan locked mid-session.
- The guard never allows: it denies or stays silent, so Claude Code's permission rules and the
  holder's prompt always still apply.
- `loom knowledge`, `loom plan summary seal`, `loom map` and `loom knowledge context` work inside the
  sandbox; raw writes to code and `.git` do not.
- Non-goals: launching sessions, the spool events, `PermissionRequest` answers (plan 7); the
  pressure run itself (plan 11); a second locked-set rule (plan 1 owns it).

## Decisions

1. **New crate module `loom/src/plan_session/`** (`pub mod plan_session;` in `lib.rs`) with
   `kind`, `sandbox`, `guard`, `hook_cli`. Plan 7 adds the bridge modules beside them. The
   builder sits beside `sandbox::settings` and calls none of `generate_settings_json`
   (`sandbox/settings.rs:205`), whose cache-write grants (`policy.rs:165-167`), orchestration
   reads and plan config a planning session must not have. It reuses `policy::mandatory_deny_read`
   (widened to `pub(crate)` with its two enclosing `mod` lines; no line count changes in the
   ledger-tracked `sandbox/settings.rs`), `state_root::token_deny_paths` and `resolve_state_root`,
   the `safe_fs` 0600 write pattern of `write_capsule_file` (`native/session_settings.rs:218-257`,
   itself tied to `capsules/`), and the hooks object shape of `capsule_hooks`
   (`session_settings/contents.rs:178-234`).
2. **The guard's record is `guard.json`**, written 0600 beside `settings.json` in the session
   directory (section 07's layout plus this one file). It carries the kind, the project root, the
   runtime directory, the session directory and the home; the session cannot write or read it
   (`~/.loom/plan` is denied in every layer). Plan 7's `PermissionRequest` hook reads the kind from
   the same file.
3. **The hook command is `loom plan hook <event> --session-dir <dir>`**: the running binary's
   absolute path and the session directory, shell-escaped, in the settings file. `event` is a
   positional string so an unknown or mismatched event exits 0 silently instead of clap's exit 2.
   The command is hidden; it gets a README line and a `loom-usage` sentence but no completions
   (metaplan "Doctrine sync").
4. **Defer means silence.** A guard that printed `permissionDecision: "allow"` would skip Claude
   Code's permission rules for that call, voiding layer 2 and the holder's prompt. The guard prints
   only denies.
5. **Summary sessions get a narrower OS layer than the spec's authoring example**: every `doc/`
   entry except `loom`, and every `doc/loom/` entry except `summaries`, is write-denied, so only
   drafts and `seal` remain. Section 12's table gives summary sessions no `doc/` writes; the
   authoring layout would leave them OS-writable.
6. **Reads.** The guard denies a Read, Grep or Glob of any `denyRead` entry (the same list, from
   one function) unless it is one of the session's own grants, and a Grep or Glob rooted at a
   strict ancestor of a home-side deny (a credential path, `~/.loom/plan`). A search of the project
   root stays allowed: the daemon's `.ignore` at the state root (`state_root.rs:94`) keeps ripgrep
   out of the token files. Reads are matched on the lexical path AND its canonical form, and any
   `/proc` or `/dev/fd` path is denied, so a symlink under `doc/`, a symlinked ancestor of the
   project root or `/proc/self/cwd` cannot reach a secret. The runtime deny is the canonical parent
   of the server's runtime directory (so `LOOM_HOME` is followed) plus `~/.loom/plan`.
   File-tool writes under Claude Code's per-user temporary directory (`<TMPDIR or /tmp>/claude-<uid>/`)
   are allowed for every kind, the last row of section 12's write table; the stage confirms the
   directory name against the installed Claude Code.
7. **The confinement test is an integration test** (`loom/tests/plan_session_confinement.rs`)
   with its own small srt runner modelled on `native/tests_confinement_srt.rs`: that helper is an
   in-crate `cfg(test)` module an integration test cannot reach, and the probes need the built
   binary (`CARGO_BIN_EXE_loom`). It skips with its reason where `bwrap`, `socat` or `srt` is
   missing or `srt -c true` fails, through `skip_unless`, so `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`
   turns a skip into a failure. It proves both the grants and the denies (runtime lock and
   answers, a credential, the project token, loopback TCP, a Unix socket), with no fallback layout.
   It cannot run in the criteria runner or the session (nested sandbox, no Unix sockets), so
   integration-verify ends with an operator block that runs it on the host with the variable set.
9. **Tool denies.** Every kind denies the stage capsule's `SESSION_TOOL_DENIES` (`LSP`,
   `RemoteTrigger`, `CronCreate`, `PushNotification`) and `Workflow`, reusing the constants;
   authoring and summary also deny `SendMessage` and `ListAgents`. Section 12's "Reused from
   stages" table omits them; the proposed spec row is filed with the lead.

## Environment inventory

- `srt` (`@anthropic-ai/sandbox-runtime` 0.0.78), `bwrap` and `socat` on the host PATH before
  `loom run`: the operator installs srt (`bun add -g @anthropic-ai/sandbox-runtime@0.0.78`, or a
  PATH shim `exec bunx @anthropic-ai/sandbox-runtime@0.0.78 "$@"`); `planning-sandbox`'s
  `before_stage` refuses to start without it, so the confinement worker reads srt's real schema.
- The operator's confinement run at the end of integration-verify (Linux; macOS by the operator
  on a Mac with the same command).
8. **No knowledge-bootstrap.** The sandbox, settings and hook knowledge this plan needs is in
   `architecture/execution-containment.md`, `architecture/security-and-isolation.md`,
   `mistakes/sandbox-and-settings.md` and `mistakes/sandbox-write-rules-inert.md`; every seam is
   cited by symbol and line (read at `b0f91c00`) in the briefs.
9. **Gate per stage.** The standard stage runs `cargo build`, its own module filters, its contract
   and integration targets, the maintainability scan, clippy and fmt; the full
   `cargo test --all-targets`, the doc tests and `cargo doc -D warnings` run in integration-verify,
   which carries the metaplan's whole pre-push gate.

## Execution Diagram

```mermaid
graph LR
    planning-sandbox --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. `planning-sandbox`: kinds, settings builder, guard, hook command, confinement test

One stage; the Stage Necessity Test finds no merge-order, file-conflict, checkpoint or context
reason to split it (Q1-Q4 all no: the guard needs the builder's types, a compile-order dependency
handled by waves). Lanes are mixed (`claude` first): Claude keeps the security core, W1 (opus,
the settings builder), W2 (opus, the guard) and W3 (sonnet, the real-sandbox confinement test,
which codex cannot run); eleven Codex units at effort high (Sol 6.1 for C2, C3, C8 and C9; Terra 6.1 for the other units) write the kinds, the
snapshot and guard tests, the hook entry and CLI wiring, and the two doc lines, in five waves.

Risk checklist and the contracts covering each:

- Untrusted input (the hook payload): `guard-fails-closed-on-garbage`.
- Filesystem paths and symlinks: `guard-denies-symlinked-component`, `no-deny-nested-under-denied-dir`,
  `permission-rules-absolute-and-edit-only`.
- Configuration propagation (the kind): `summary-kind-has-no-web-or-doc-write`,
  `pressure-allows-plan-bundle-marker-only`.
- Lifecycle (a plan locked mid-session): `guard-denies-plan-locked-mid-session`.
- Reachability: `hook-cli-reachable` drives the built binary.
- The behaviour the stage exists for: `authoring-denies-every-top-level-but-doc`,
  `assert-refuses-escape-hatches`, `guard-defers-silently-on-allowed-write`,
  `guard-denies-runtime-secrets-allows-drafts`.
- Process I/O volume: the payload cap (`guard-fails-closed-on-garbage` includes a 1 MiB + 1 payload).

### Integration Verification

The metaplan's full gate from the repository root, a security review against section 12's threat
model rows for planning sessions, an architecture review against `common.md`, a smoke run of the
built binary as a hook, and an operator block for the real-sandbox confinement run.

### Knowledge Distillation

Records the planning sandbox as a tier-2 topic linked from `architecture.md`, and checks the README
and `loom-usage` lines W3 wrote.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: planning-sandbox
      name: "Planning session settings builder and PreToolUse guard"
      summary: "Adds loom::plan_session: the per-kind settings file for planning sessions (OS sandbox lists, permission rules, hook registration), the loom plan hook PreToolUse path guard that refuses locked plans, code, symlinks and secrets, and a confinement test inside a real sandbox."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      implementers: ["claude", "codex"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: []
      before_stage:
        - command: "test ! -e src/plan_session"
          description: "loom::plan_session does not exist yet"
        - command: "rg -qF 'pub struct LockedSet' src/planning/paths.rs"
          description: "plan 1's shared path predicate is merged"
        - command: "command -v srt"
          description: "srt (@anthropic-ai/sandbox-runtime 0.0.78) is installed on the host for the confinement test"
      after_stage:
        - command: "rg -qF 'mod plan_session' src/lib.rs"
          description: "loom::plan_session is declared in the library"
        - command: "rg -qF 'session_dir: std::path::PathBuf' src/cli/types_ops.rs"
          description: "the hidden PlanCommands::Hook variant carries --session-dir"
      description: |
        Build stage 1 of doc/plans/PLAN-feat-planner-06-session-sandbox.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-06-session-sandbox.md once loom run starts).
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/ (contracts run as cargo test <name> -- --exact from it). YAML paths and the worker table are package-relative; brief paths are repo-relative.
        Every worker reads doc/plans/briefs/feat-planner-06-session-sandbox/common.md first (the authoritative public API of loom::plan_session, the settings table per kind, the guard's decision order, the size and safety rules), then its own brief.
        PRECONDITION: plan 1 (PLAN-feat-planner-01-documents) is merged: loom::planning::{runtime::{RuntimePaths, runtime_paths_under}, paths::{LockedSet, LockReason, DocArea, classify_write, symlink_component}, slug::plan_slug} exist. Confirm with loom map --find-all LockedSet and loom map --find-all symlink_component before spawning; if any is missing, loom stage block this stage naming it (never reimplement the predicate).
        Territories are DISJOINT. Workers NEVER spawn subagents. Spawn by agent type with the fixed prompt plus "Your brief: <path>. Read it in full before anything else." Wait with one background loom subagents watch.
        SEED (the orchestrator, before wave 1; under 20 lines): add `pub mod plan_session;` to src/lib.rs; write src/plan_session/mod.rs with a //! doc line ("Planning sessions of loom plan: kinds, the per-session sandbox settings and the PreToolUse guard; the bridge modules of plan 7 join here.") and `pub mod kind; pub mod sandbox; pub mod guard; pub mod hook_cli;`; create src/plan_session/kind.rs, src/plan_session/sandbox/mod.rs, src/plan_session/guard/mod.rs, src/plan_session/hook_cli.rs and src/commands/plan/hook.rs each holding only its //! purpose line, add `pub mod hook;` to src/commands/plan/mod.rs, and declare `#[cfg(test)] mod tests; #[cfg(test)] mod tests_kinds;` in sandbox/mod.rs and `#[cfg(test)] mod tests; #[cfg(test)] mod tests_reads;` in guard/mod.rs with purpose-line files for those four (so no worker creates a file a codex unit owns), so the crate compiles. Each seeded file is then owned by exactly one worker or unit below.
        LANES (the user's decision; do not ask): codex units C1-C11 go through loom-codex-forwarder subagents spawned in the FOREGROUND, at most 6 at once over disjoint files, each with --model gpt-6.1-sol for C2, C3, C8, C9 and --model gpt-6.1-terra for the other units, always with --effort high and an explicit Bash timeout of 600000 ms; the forwarded text is the unit's block in doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/codex-units.md, verbatim. Never use codex:codex-rescue. Codex units never run git and run compile checks only: after EACH unit run its proof command and git status --short yourself, and route a failure to a fresh unit (re-split if it timed out) or, after one failed retry, to a sonnet loom-software-engineer. Claude keeps the security core: the settings builder (W1), the guard (W2), and the real-sandbox confinement test (W3, which codex cannot run).
        WAVE 1: C1 (kind.rs). WAVE 2: W1 alone (opus). WAVE 3, in ONE message after W1 returned: W2 (opus) and W3 (sonnet) as Claude subagents, and C2, C3, C10, C11 as codex forwards. WAVE 4 after W2 returned: C4, C5, C6, C8, C9 (codex). WAVE 5: C7 (needs the variant C6 added). Claude workers and codex forwards of one wave may run together; the 6-forward cap counts codex only.
        AFTER THE WAVES: rustfmt --check --edition 2021 tests/plan_session_sandbox_contracts.rs (a frozen file the formatter would rewrite fails completion; a failure is a dispute-contract, never an edit); then cargo fmt --all; then cargo test --test maintainability; route any failure to a fresh worker of the owning territory; then one loom check.
        LEDGER: no row of maintainability-baseline.txt changes. src/sandbox/settings.rs (ledger row 438) gets a one-token visibility change only, so its count stays exact.
        MUTATION: before the final review round, for each contract apply the wrong implementation its rejects: text names, confirm the contract fails, restore from a saved copy (never git checkout), and record loom memory note "mutation: <contract id> red under <mutation>".

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | W1 | Settings builder | opus | src/plan_session/sandbox/mod.rs; src/plan_session/sandbox/filesystem.rs; src/plan_session/sandbox/permissions.rs; src/plan_session/sandbox/reads.rs; src/plan_session/sandbox/assert.rs; src/sandbox/mod.rs; src/sandbox/settings.rs; src/sandbox/settings/policy.rs; src/orchestrator/terminal/native/mod.rs; src/orchestrator/terminal/native/session_settings.rs; src/orchestrator/terminal/native/session_settings/contents.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/w1-settings.md |
        | W2 | Guard core | opus | src/plan_session/guard/mod.rs; src/plan_session/guard/paths.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/w2-guard.md |
        | W3 | Confinement test | sonnet | tests/plan_session_confinement.rs; tests/plan_session_confinement/srt.rs; tests/plan_session_confinement/fixture.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/w3-confinement.md |
        | C1 | Codex unit C1: SessionKind, SessionDir | codex terra/high | src/plan_session/kind.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/codex-units.md |
        | C2-C3 | Codex units C2, C3 (one forward per file): settings snapshot and per-kind tests | codex sol/high | src/plan_session/sandbox/tests.rs; src/plan_session/sandbox/tests_kinds.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/codex-units.md |
        | C4-C7 | Codex units C4, C5, C6, C7 (one forward per file): hook entry and CLI wiring | codex terra/high | src/plan_session/hook_cli.rs; src/commands/plan/hook.rs; src/cli/types_ops.rs; src/cli/dispatch.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/codex-units.md |
        | C8-C9 | Codex units C8, C9 (one forward per file): guard tests | codex sol/high | src/plan_session/guard/tests.rs; src/plan_session/guard/tests_reads.rs | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/codex-units.md |
        | C10-C11 | Codex units C10, C11 (one forward per file): doc lines | codex terra/high | ../README.md; ../skills/loom-usage/SKILL.md | common.md | doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/codex-units.md |

        CONTRACT SURFACE (the contract session writes tests/plan_session_sandbox_contracts.rs and its helper tests/plan_session_sandbox_contracts/support.rs from this text before any code; every contract is a top-level #[test] fn of at most 50 lines, each file at most 400 lines, rustfmt-clean; fixtures are built in tempfile::Builder::new().prefix("loom-plan-session-").tempdir_in(std::env::temp_dir()) trees with a fake home and a fake loom home inside them):
        - loom::plan_session::kind::{SessionKind::{Authoring, Summary, Pressure { plan: PathBuf }}, SessionDir::new(PathBuf), SessionDir::{settings, guard, marker, done}(&self) -> PathBuf}
        - loom::plan_session::sandbox::{SandboxInputs { project_root, runtime, session_dir, kind, loom_bin, home }, HookSpec { event, matcher, timeout_secs }, build_settings(&SandboxInputs, &LockedSet, &[HookSpec]) -> anyhow::Result<serde_json::Value>, assert_planning_settings(&Value) -> anyhow::Result<()>, write_session_sandbox(&SandboxInputs, &[HookSpec]) -> anyhow::Result<SandboxFiles { settings, guard }>}
        - loom::plan_session::guard::run_pre_tool_use(session_dir: &Path, stdin: impl Read) -> Option<String> (None = no output; Some = one JSON deny line)
        - loom::planning::{runtime::runtime_paths_under(loom_dir: &Path, root: &Path) -> anyhow::Result<RuntimePaths>, paths::LockedSet::scan(root: &Path) -> anyhow::Result<LockedSet>} (plan 1, merged)
        - the binary: env!("CARGO_BIN_EXE_loom") bound to a const and spawned through a local loom_cmd() that removes every LOOM_* variable and sets LOOM_HOME to a temp dir (tests/integration/binary_spawn_guard.rs rule); argv plan hook <Event> --session-dir <dir>, payload on stdin.
        Settings JSON paths the contracts read: /sandbox/filesystem/{denyWrite,allowWrite,denyRead,allowRead}, /permissions/{allow,ask,deny,defaultMode,disableBypassPermissionsMode}, /sandbox/{enabled,failIfUnavailable,allowUnsandboxedCommands,autoAllowBashIfSandboxed,network}, /hooks/PreToolUse/0/hooks/0/command. Permission rule paths are Edit(/ + absolute path + ...), i.e. Edit(//abs/...).
        The contracts are red at freeze because loom::plan_session does not exist.

        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/lib.rs"
        - "src/plan_session/**"
        - "src/sandbox/mod.rs"
        - "src/sandbox/settings.rs"
        - "src/sandbox/settings/policy.rs"
        - "src/orchestrator/terminal/native/mod.rs"
        - "src/orchestrator/terminal/native/session_settings.rs"
        - "src/orchestrator/terminal/native/session_settings/contents.rs"
        - "src/commands/plan/hook.rs"
        - "src/commands/plan/mod.rs"
        - "src/cli/types_ops.rs"
        - "src/cli/dispatch.rs"
        - "tests/plan_session_sandbox_contracts.rs"
        - "tests/plan_session_sandbox_contracts/**"
        - "tests/plan_session_confinement.rs"
        - "tests/plan_session_confinement/**"
        - "../README.md"
        - "../skills/loom-usage/SKILL.md"
      harness: ["tests/plan_session_sandbox_contracts/**"]
      acceptance:
        - "cargo build"
        - "cargo test --lib plan_session::"
        - "cargo test --lib sandbox::settings::"
        - "cargo test --test plan_session_sandbox_contracts"
        - "cargo test --test plan_session_confinement"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
      artifacts:
        - "src/plan_session/sandbox/mod.rs"
        - "src/plan_session/sandbox/filesystem.rs"
        - "src/plan_session/sandbox/permissions.rs"
        - "src/plan_session/guard/mod.rs"
        - "src/plan_session/hook_cli.rs"
        - "tests/plan_session_confinement.rs"
      wiring:
        - source: "src/cli/dispatch.rs"
          pattern: "PlanCommands::Hook"
          literal: true
          description: "loom plan hook is dispatched"
        - source: "src/plan_session/hook_cli.rs"
          pattern: "run_pre_tool_use("
          literal: true
          description: "the hook entry calls the guard for PreToolUse"
        - source: "src/plan_session/sandbox/mod.rs"
          pattern: "assert_planning_settings("
          literal: true
          description: "write_session_sandbox asserts every settings file before writing it"
      contracts:
        - id: authoring-denies-every-top-level-but-doc
          file: tests/plan_session_sandbox_contracts.rs
          test: authoring_settings_deny_every_top_level_entry_but_doc
          scenario: "a root holding .git/, loom/, README.md, .hidden (a dotfile), .claude/ and doc/; build_settings for SessionKind::Authoring; reads /sandbox/filesystem/denyWrite"
          rejects: "a builder with a hard-coded deny list (.git, loom) or one that skips dotfiles: <root>/.hidden and <root>/.claude must be denied and <root>/doc must not be"
        - id: no-deny-nested-under-denied-dir
          file: tests/plan_session_sandbox_contracts.rs
          test: locked_set_collapses_to_directories_with_no_nested_deny
          scenario: "a fixture with archive/DONE-PLAN-c.md, IN_PROGRESS-PLAN-b.md, REVIEW-PLAN-b.md and the bundle of plan b holding x.md and y.md (briefs/b, all under the fixture's plans directory); authoring build_settings"
          rejects: "a builder that lists every LockedSet::locked_paths file: denyWrite must hold the archive directory and the briefs/b directory themselves, and no entry may lie beneath another entry"
        - id: permission-rules-absolute-and-edit-only
          file: tests/plan_session_sandbox_contracts.rs
          test: permission_rules_are_absolute_edit_rules_with_no_write_or_read_deny
          scenario: "authoring and pressure build_settings on a fixture with doc/notes/; every string in /permissions/allow and /permissions/deny that starts with Edit( or Read( is inspected"
          rejects: "rules written with a single leading slash (resolving against the settings directory), any Write( rule, any Read( entry in deny, or an Edit deny under doc/loom/knowledge or doc/loom/summaries"
        - id: summary-kind-has-no-web-or-doc-write
          file: tests/plan_session_sandbox_contracts.rs
          test: summary_settings_allow_only_drafts_and_summary_commands
          scenario: "build_settings for SessionKind::Summary on the authoring fixture"
          rejects: "a builder that ignores the kind and emits the authoring rules: allow must contain no Edit(//<root>/doc rule, no WebSearch, and Bash(loom plan summary *) as its only Bash rule; deny must contain WebSearch, WebFetch, LSP, RemoteTrigger, CronCreate, PushNotification, Workflow, SendMessage and ListAgents; denyWrite must contain <root>/doc/product"
        - id: pressure-allows-plan-bundle-marker-only
          file: tests/plan_session_sandbox_contracts.rs
          test: pressure_settings_write_only_the_plan_its_bundle_omissions_and_marker
          scenario: "a fixture whose plans directory holds PLAN-a.md, PLAN-z.md and the bundles briefs/a and briefs/z, plus a product directory; build_settings for SessionKind::Pressure with plan = the repo-relative path of PLAN-a.md"
          rejects: "a pressure kind that reuses the authoring write set: allowWrite must equal [<session dir>/done], allow must contain Edit for PLAN-a.md, the briefs/a bundle (/**), planning-omissions.md and exactly Bash(touch <marker>), and denyWrite must contain PLAN-z.md, briefs/z and doc/product"
        - id: assert-refuses-escape-hatches
          file: tests/plan_session_sandbox_contracts.rs
          test: assert_planning_settings_refuses_each_escape_hatch
          scenario: "a built authoring settings value passes assert_planning_settings; then four copies with defaultMode bypassPermissions, sandbox.enabled false, allowUnsandboxedCommands true, and network.allowedDomains [\"example.com\"] each fail it"
          rejects: "an assert that checks only one of the keys and passes the other three"
        - id: guard-denies-plan-locked-mid-session
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_denies_edit_of_a_plan_locked_after_spawn
          scenario: "write_session_sandbox (authoring) with doc/plans/PLAN-x.md present; rename it to doc/plans/IN_PROGRESS-PLAN-x.md; run_pre_tool_use with an Edit payload whose file_path is the absolute IN_PROGRESS path"
          rejects: "a guard that decides from the lists written at spawn instead of scanning the tree on each call, and so returns None"
        - id: guard-denies-symlinked-component
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_denies_write_through_a_symlinked_directory_under_doc
          scenario: "doc/notes is a symlink to a directory outside the root; run_pre_tool_use with a Write payload for <root>/doc/notes/a.md (authoring)"
          rejects: "a guard that canonicalizes the path or checks only the final component and so allows the write"
        - id: guard-defers-silently-on-allowed-write
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_prints_nothing_for_an_allowed_doc_write
          scenario: "authoring session; run_pre_tool_use with an Edit payload for <root>/doc/product/x.md and hook_event_name PreToolUse"
          rejects: "a guard that answers permissionDecision allow (which would skip Claude Code's permission rules): the result must be None"
        - id: guard-fails-closed-on-garbage
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_denies_unparsable_oversized_and_policyless_payloads
          scenario: "run_pre_tool_use with stdin b\"not json\"; with a 1 MiB + 1 byte payload; and with a valid Edit payload against a session dir that has no guard.json"
          rejects: "a guard that exits silently (None) on a parse error, a truncated read or a missing policy"
        - id: guard-denies-runtime-secrets-allows-drafts
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_denies_reads_of_the_runtime_lock_and_allows_drafts
          scenario: "authoring session; Read payloads for <runtime>/lock, <runtime>/sessions/<id>/answers/a.json and <runtime>/drafts/x.md"
          rejects: "an all-or-nothing rule on the runtime directory: the first two must be denied and the drafts read must be None"
        - id: authoring-denies-capsule-tools
          file: tests/plan_session_sandbox_contracts.rs
          test: authoring_and_pressure_settings_deny_the_capsule_tools
          scenario: "build_settings for Authoring and for Pressure on the authoring fixture; reads /permissions/deny"
          rejects: "a builder that drops the tool denies every stage capsule carries: both must deny LSP, RemoteTrigger, CronCreate, PushNotification and Workflow; authoring must also deny SendMessage and ListAgents, and pressure must deny neither of those two (its teammates need them)"
        - id: runtime-deny-follows-loom-home
          file: tests/plan_session_sandbox_contracts.rs
          test: read_deny_covers_a_runtime_under_a_redirected_loom_home
          scenario: "a runtime from runtime_paths_under(<tmp>/other-loomhome, root) outside <fake home>/.loom; build_settings (authoring) and run_pre_tool_use with a Read of <that runtime>/lock and of <that runtime>/drafts/x.md"
          rejects: "a deny written from the literal ~/.loom/plan: /sandbox/filesystem/denyRead must hold the canonical <tmp>/other-loomhome/plan and <fake home>/.loom/plan, the lock read must be denied and the drafts read must be None"
        - id: guard-denies-read-through-symlink
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_denies_read_and_grep_through_a_symlink_under_doc
          scenario: "doc/notes is a symlink to <fake home>/.ssh holding id_rsa; run_pre_tool_use with Read of <root>/doc/notes/id_rsa and with Grep whose path is <root>/doc/notes"
          rejects: "a guard that matches the lexical path only and so defers both: each must produce a deny line"
        - id: guard-denies-proc-self-cwd-token-read
          file: tests/plan_session_sandbox_contracts.rs
          test: guard_denies_reads_through_proc_and_a_symlinked_root
          scenario: "Read of /proc/self/cwd/.loom/work/admin.token; and a project root reached through a symlinked ancestor (<tmp>/link -> <tmp>/real) with a Read of <tmp>/link/proj/.loom/work/user.token while the policy names <tmp>/real/proj"
          rejects: "a guard that trusts the lexical form: both reads are denied, while a Read of <tmp>/link/proj/loom/src/lib.rs returns None"
        - id: hook-cli-reachable
          file: tests/plan_session_sandbox_contracts.rs
          test: loom_plan_hook_answers_through_the_binary
          scenario: "the built binary run as plan hook PreToolUse --session-dir <dir without guard.json> with stdin {} and as plan hook Stop --session-dir <dir> with stdin {}"
          rejects: "an unwired command or a clap ValueEnum event: the first run must exit 0 with a deny JSON line on stdout, the second must exit 0 with empty stdout"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Confirms the planning sandbox builder and guard work on the merged tree, pass a security review against section 12, and that the full suite passes."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "."
      dependencies: ["planning-sandbox"]
      description: |
        Final verification of doc/plans/PLAN-feat-planner-06-session-sandbox.md (doc/plans/IN_PROGRESS-PLAN-feat-planner-06-session-sandbox.md while running). Verify FUNCTIONAL INTEGRATION, not only tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan, doc/plans/briefs/feat-planner-06-session-sandbox/common.md, loom memory show --all, and the knowledge sections the plan cites. working_dir is the repository root.
        BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below, through one loom check.
        FUNCTIONAL (from this session's shell): build a scratch project under the worktree's scratch area (a git repo with doc/plans/PLAN-a.md and doc/plans/IN_PROGRESS-PLAN-b.md), write an authoring sandbox for it with a tiny cargo test or the contract helper, then pipe a hand-written Edit payload for the IN_PROGRESS plan into cargo run -q --manifest-path loom/Cargo.toml -- plan hook PreToolUse --session-dir <dir> and confirm the deny line; the same for doc/product/x.md prints nothing. Record in loom memory that tests/plan_session_confinement.rs skipped here (it cannot run in a nested sandbox).
        CODE REVIEW: parallel loom-code-reviewer subagents. Security, judged against doc/product/section_12.md "Planning sessions" (write table, three layers, Commands, Network, No way for a session to approve itself, Pressure sessions) and the threat-model rows for "A planning session, mistaken or steered by injected text": every settings entry a literal existing path with no nested deny; Edit(//...) only, no Write( or Read( deny, no Edit deny under doc/loom; the guard never prints allow, scans the locked set on every call, refuses symlinked components and non-regular targets, fails closed on every error path and panic; reads of the runtime dir, credentials and daemon tokens refused; host-side opens through safe_read/safe_fs. Architecture: every API in common.md exists with the published signature (plan 7 binds to it); no second locked-set rule; generate_settings_json not called. Test coverage: one guard test per write-table row and kind, the mutation notes the stage recorded. Fix or dispute every finding; never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each implemented one with loom memory resolve <id> --outcome implemented --reason <what changed>.
        OPERATOR RUN (last, after the gate is green and every finding is closed; the confinement test needs a real host sandbox): file loom stage block integration-verify "Operator: from the repository root, with srt 0.0.78, bwrap and socat on PATH, run LOOM_TEST_REQUIRE_SANDBOX_FREE=1 cargo test --manifest-path loom/Cargo.toml --test plan_session_confinement -- --nocapture (and the same on a Mac when one is available), then record loom memory note \"confinement: <ran|failed>, srt <version>, <os>, <failing probes>\" and loom stage retry integration-verify". On retry read that memory: a failed probe is a finding to fix; a missing memory is a re-block.
        Record discoveries to loom memory for knowledge-distill, including any knowledge file the tree contradicts: loom memory note "stale-knowledge: ...".
      acceptance:
        - "cargo build --manifest-path loom/Cargo.toml"
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo test --manifest-path loom/Cargo.toml --doc --workspace --all-features -- --test-threads=1"
          timeout_secs: 1200
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
        - "printf 'not json' | cargo run -q --manifest-path loom/Cargo.toml -- plan hook PreToolUse --session-dir loom/target/no-such-session | rg -qF '\"permissionDecision\":\"deny\"'"
      wiring:
        - source: "loom/src/cli/dispatch.rs"
          pattern: "PlanCommands::Hook"
          literal: true
          description: "loom plan hook is dispatched"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records the planning session sandbox (settings per kind, the guard, guard.json) in the knowledge base."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; keep code spot-reads narrow.
        START with loom memory pending --group; read the plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update.
        Then curate: a new tier-2 topic architecture/planning-session-sandbox (loom::plan_session: SessionKind and SessionDir, the three layers per kind and why summary sessions deny doc/ entries, the literal-path and no-nested-deny rules, Edit(//...) only, guard.json, the guard's decision order, why it never prints allow, the read-deny ancestor rule and its reliance on the state-root .ignore, the hook command form, the confinement test and its srt skip), with a 2-4 line summary and link in architecture.md, and loom knowledge annotate architecture/planning-session-sandbox --blurb "Planning sandbox: settings per kind, guard". Add a line to architecture/execution-containment.md's section on stage settings naming the planning builder as the second settings producer, with a link. Record in concerns.md the accepted limits section 12 lists that the implementation confirmed or found (Bash writes inside doc/loom/, new top-level paths, mid-session locks for Bash) if the stage memories report any difference. TIER ROUTING as usual.
        DOCS: README.md "Plan Commands" and skills/loom-usage/SKILL.md already carry the loom plan hook line (stage planning-sandbox); correct them only if the tree disagrees.
        INDEX BUDGET: doc/loom/knowledge/INDEX.md must stay at or under 15,000 bytes after this stage's writes (plan 1 trims it to that). Keep every blurb you write under 50 characters; shorten the longest existing blurbs with loom knowledge annotate <category>/<slug> --blurb "<shorter>" if a write pushes it over; never delete a topic to save bytes; never run loom knowledge check --write-baseline to absorb an INDEX size issue.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then resolve it promoted, merged or discarded.
        RECEIPTS: every memory taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict and resolve whatever it lists.
        LAST, if this stage removed structural issues, ratchet the baseline: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - 'test "$(wc -c < doc/loom/knowledge/INDEX.md)" -le 15000'
        - "test -s doc/loom/knowledge/architecture/planning-session-sandbox.md"
        - "rg -qF 'planning-session-sandbox' doc/loom/knowledge/architecture.md"
      files: ["doc/loom/knowledge/**", "README.md", "skills/loom-usage/SKILL.md"]
```

<!-- END loom METADATA -->
