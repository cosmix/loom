# Plan: Live profile and confinement hardening

## Overview

A few of loom's own tests drive a real Claude Code: plan 07's bridge run, plan 08's four
guidance runs, plan 09's chat run and plan 11's handoff runs. They need outbound network and a
credential. Loom's confined runner (the daemon's host job behind `loom check` and
`loom stage complete`) gives neither, and the Claude Code session sandbox gives less still, so
these tests have been run unconfined on a host or not at all.

This plan adds a live profile: the same confined runner and filesystem policy, a private
writable `HOME`, and one gated outbound path. On Linux the runner's empty network namespace
gains exactly one read-only bind, the directory holding a per-run gate's Unix socket, and a
`loom live exec` forwarder inside listens on loopback so clients use `HTTPS_PROXY`. On macOS
the Seatbelt profile gains outbound access to the gate's loopback port only. The gate, on the
host, requires a per-run secret, terminates TLS for allowlisted hosts with a per-run CA, swaps
in the real credential, re-originates TLS and refuses everything else; the credential never
enters the sandbox. A stage declares `live:` entries; they run only through
`loom check --live <stage>`, which records a receipt bound to the tree's content. `loom stage
complete` never runs them: a `required` entry needs a passing receipt for the current content,
and an advisory one is reported. A merge resolution changes that content, so a merge resolver
of a stage with required entries requests the same run for the resolved tree before it reports
the merge resolved.

The same plan closes escape hatches in loom's own sessions (display and session-bus variables in
confined commands, `command_confinement: inherit`, `allow_unsandboxed_escape`, writable codex
trust surfaces), makes `loom plan verify` refuse escape flags in plan-authored commands, and
replaces `LOOM_TEST_REQUIRE_SANDBOX_FREE` with per-facility test guards and a live-profile guard.

The plan runs after the planning-UI plans 08-11 merge and before the autonomous-recovery plan
(`doc/plans/PLAN-autonomous-recovery.md`, whose briefs assume this plan's hardening has merged).

## Goals

- A stage proves behaviour that needs a real Claude Code through a `live:` entry; a machine
  without the profile's prerequisites reports `unavailable: <reason>`, never a pass, never a
  silent skip, never a host fallback.
- The sandbox never holds the real credential; the gate is the only network path and serves the
  allowlisted hosts only, to a client that knows the run's secret.
- Plan-authored commands always run confined; no plan field, flag or variable widens that.
- Tests that need the live profile are counted in the output of every ordinary run that executes
  them, passing or failing (a cached pass replays only a short output tail and shows no count;
  Decision 10), and a probe for a facility the environment claims to provide fails instead of
  skipping, so coverage never disappears silently.
- Everything is general: Linux and macOS, any user, any project, no root, nothing specific to
  this machine or repository.
- Non-goals: a VM tier (Follow-ups); live entries for the operator tests in this plan's own
  YAML (the installed loom that parses this plan at `loom init` does not know `live:`;
  Follow-ups carries the entries); codex credentials (plan 11's tests report unavailable until
  then).

## Cross-plan interfaces

Every producer below has merged before this plan runs. Names come from committed code
(verified on 2026-10-28 at `8e568d8c`) or from the producing plan's stage YAML and briefs where
the code is not yet written; each stage confirms them with `loom map` before briefing.

| Role | Producer | Name used here |
| --- | --- | --- |
| Operator end-to-end test | plan 07 (merged) | `loom/tests/plan_session_bridge_e2e/operator.rs` `operator_end_to_end_with_claude_code` (`#[ignore]`; reads `HOME` from the environment; `TrustGuard` calls `untrust_worktree`, which resolves `dirs::home_dir()`, i.e. `$HOME` first) |
| Guidance operator tests | plan 08 (merged, `2d96ecea`) | `loom/tests/planning_guidance_operator.rs`: four `#[ignore]` tests `operator_{vision,section,metaplan,plan}_session_loads_its_skill` sharing `fn operator(...)` |
| Chat operator test | plan 09 YAML (chat-server `files:`), brief w3-routes | `loom/tests/plan_chat_operator.rs`, test `real_session_attach_send_restore` (not written at authoring) |
| Handoff operator tests | plan 11 YAML (app-handoff `files:`), brief a1-pressure-driver | `loom/tests/plan_handoff_operator.rs`: `operator_pressure_run_records_one_line`, `operator_stopped_run_writes_no_record`, `operator_commit_and_start_run` (they also drive codex) |
| Plan 11 edits | plan 11 YAML | `src/plan_session/sandbox/tests_kinds.rs` (this plan edits the sibling `tests_kinds_fixture.rs`) |
| Plan 08 edits | plan 08 YAML | `../skills/loom-plan-writer/SKILL.md` (edited again by `live-checks` R4) |
| Consumer after this plan | autonomous-recovery briefs | expects `CommandConfinement::Inherit` gone, `allow_unsandboxed_escape` gone, `apply_confined_host_environment`, `host-os-boundary-v10`; it edits `confine_host.rs` `read_roots`, `control_block.rs`, the dispute field enums, `signals/merge.rs` (keeping the live section call) and `signals/mod.rs` after this plan |

First-stage fail-fast: each stage's `before_stage` checks the files its premise needs, so a
false premise stops the plan before any work.

## The runner, read from the code

`verify/criteria/confine_host.rs` builds every criterion's boundary. Confirmed at authoring:

- Linux: `bwrap --die-with-parent --unshare-all --new-session --proc /proc --dev /dev --tmpfs
  /tmp --tmpfs /var/tmp` (lines 265-279), `--share-net` only for provision, then binds of the
  tree, scratch and Cargo home, read-only binds of `read_roots`, the git roots, the protected
  paths, the resolver file's target alone (`bind_resolver_target`), and `--remount-ro /`. No
  seccomp filter, so `AF_UNIX` works under the private `/tmp`, and loopback TCP works in the new
  network namespace. Outbound connects fail (no route). `HOME` is never bound and `/run` is never
  bound, so `HOME` is not writable and the systemd-resolved stub (`127.0.0.53` in a foreign
  namespace) is unreachable: DNS fails in the real runner. The probe's DNS success came from its
  `--ro-bind / /`.
- `read_roots` (lines 130-171) exposes system directories, `~/.rustup`, `~/.bun/bin`,
  `~/.bun/install/cache`, the uv caches, Cargo's `bin`/`registry`/`git`, and `LOOM_BIN`. A
  Bun-global tool's symlink target under `~/.bun/install/global` is not exposed, so `srt` looks
  missing inside; `~/.local/bin` and `~/.local/share/claude` are not exposed, so a native
  Claude Code is invisible too. The live profile binds the canonical Claude Code binary itself.
- `tests/plan_session_confinement.rs`'s header says the criteria runner "nest[s] a sandbox that
  cannot start srt". Refuted as a nesting claim: nothing in the runner's flags blocks a nested
  user namespace (the measured nested bwrap exited 0). Inside the runner srt is skipped because
  its install's symlink target is outside the read roots, the gap the autonomous-recovery
  plan's tool-root work closes. The claim holds for a Claude Code session sandbox, which forbids
  user namespaces. `test-escape-policy` rewrites the header.
- macOS: `sandbox-exec -p` with `(deny default)`, outbound network only for provision; Unix
  sockets and loopback are denied; a nested `sandbox-exec` is refused.

Every boundary fact the live profile relies on is proven by a contract that drives the real
wrapping (`live-gate-and-boundary`), never assumed from the probe.

## Decisions

1. **Live entries run only on request.** `loom check --live <stage>` relays from a session to
   the daemon as a new `JobKind::Live` host job (`HostJobRequest.live`); in the operator's shell
   it runs locally. It runs only the stage's live entries, in the profile, and writes
   `verification/<stage>/live-receipt.json`. `loom stage complete` never runs them; its `live`
   check reads the receipt. A new job kind (not a flag on `Check`) keeps the submit rules,
   deadline and apply step explicit: a live run never joins a check, never caches, and changes
   no stage state.
2. **Receipt binding.** The receipt binds the checkout's content manifest digest (the criteria
   cache's manifest: tracked and untracked non-ignored files, commit identity left out, so a
   dirty-tree run matches the commit of the same bytes). When the manifest cannot be taken (a
   tracked symlink, a submodule, a flagged index entry) the binding is the HEAD tree id and the
   run needs a clean tree. Each entry's outcome carries its own definition hash, so amending one
   entry keeps the others' outcomes. A tree that changed during the run voids the binding.
3. **Completion rule.** For each `required: true` entry: a matching `passed` outcome, else the
   `live` check fails with one instruction (`run loom check --live <stage>`), the entry's
   failure, or its unavailability. Entries with `required: false` (the default) are advisory:
   their outcomes are printed in the completion output and in `loom live status --stage`.
4. **Unavailable is its own outcome.** No credential: every entry `unavailable (credential)`,
   nothing runs. No bubblewrap or sandbox-exec, a refused nested Seatbelt, or a test reporting a
   facility the platform's profile lacks: `unavailable (facility)`. Never a pass, never a skip,
   never the host.
5. **Block or dispute.** `stage_service/control_block.rs` refuses an agent's block only while
   the latest signed completion attempt has a failing `acceptance-<i>` (read at authoring). This
   plan adds: a failing `live` check blocks only when every failing required entry is
   `unavailable (credential)` for the current content (a person's need, CLAUDE.md Rule 13);
   anything else names `loom check --live` or the new `loom stage dispute-criteria <stage>
   --field live --criterion-index <n>`, whose accepted verdict amends the plan's `live` list. The
   decision lives in `live::completion::block_refusal` so the autonomous-recovery plan's guard
   rewrite calls one function.
6. **The gate.** In-process in the job (a thread with a current-thread tokio runtime). `CONNECT`
   only; `Proxy-Authorization: Basic loom-live:<secret>` required on Linux too (the socket sits
   in a 0700 directory, but every same-uid process could otherwise use it, and one header check
   keeps both platforms on one protocol); loopback-only listener on macOS (every local process
   can reach that port, so the secret is what admits a client). TLS termination with a per-run
   name-constrained CA, ALPN `http/1.1` only, client auth headers stripped, `x-api-key` (API
   key) or `authorization: Bearer` (OAuth token) injected, upstream TLS with real verification,
   streaming bodies, request-line-only logs, a per-run budget of 1000 requests, 1 GiB of
   responses and 32 MiB per request body, plus the entry's own timeout. Upstream address and
   root store are injectable (`Upstream::fixed`), so every gate test runs offline. Allowlist:
   `api.anthropic.com` (Claude Code's API, feature-flag and telemetry host); a long-lived
   `claude setup-token` token needs no refresh host. `ANTHROPIC_BASE_URL` pointed at a plain
   gate was rejected: it switches Claude Code into gateway mode, a different code path from the
   one under test.
7. **Forwarder.** `loom live exec --listen 127.0.0.1:3128 --gate <socket> -- sh -c <command>`,
   a hidden subcommand of the loom binary already bound into the runner. Chosen over `socat`:
   no host dependency (many hosts lack socat), it binds before the command starts (no readiness
   race), it exits with the command and returns its exit code, and it ships with the gate it
   talks to.
8. **Credential store.** `loom live credential set --kind api-key|oauth-token` reads one line
   from stdin into `<state root>/live-credential` (mode 0600; refused inside any loom session).
   The file joins `STATE_ROOT_SECRET_FILES`, the list every capsule writer, the credential guard
   hook and the confined runner already derive their denies from, so no session and no confined
   command reads it, wherever the state root lives. A relocated `LOOM_HOME` plays no part: the
   credential is per project, set once per repository, and removed by `loom clean --state`.
   Revisit when operators need one credential across projects: then it moves to the loom user
   directory and each capsule gets its resolved path at spawn. The operator's `~/.claude` is
   never read.
9. **HOME seeding.** Claude Code started by loom's planning launch gets `env -i` plus
   `process::agent_session_environment_from` (no proxy or CA names) and `--setting-sources
   user,project`, so the profile writes the proxy, CA, placeholder credential and
   nonessential-traffic switches into `$HOME/.claude/settings.json` `env` (Claude Code documents
   every network variable as settable there), onboarding keys into `$HOME/.claude.json`, and an
   `apiKeyHelper` script for an API key. Loom's own launch adds the project trust entry to the
   same `$HOME/.claude.json` (`fs::permissions::trust::trust_project`). The process environment
   carries the same proxy and CA variables for direct clients, plus `NO_PROXY` for loopback.
   Claude Code's embedded sandbox would chain allowed-domain traffic to `HTTPS_PROXY` (srt takes
   a parent proxy from its environment), but loom's planning sessions allow no domains to
   sandboxed commands, so nothing depends on it.
10. **Test guards.** `LOOM_TEST_FACILITIES` lists what the environment provides; loom's runner
    sets it (Linux: `unix-sockets`, `loopback`, `process-tree`, `bubblewrap` when the host can
    start bubblewrap, `tmux` when present; macOS: none), the live profile adds `network-gate`,
    `writable-home` and `claude-code`, and CI sets it per job. A probe for an advertised
    facility that fails is a failure; it is an environment variable with no marker file because
    it can only add strictness. The live-profile guard needs `LOOM_LIVE_PROFILE`, a regular
    `$HOME/.loom-live-profile` with the same run id, and a `HOME` that is not the account's
    passwd home, because it turns on tests that write `$HOME/.claude.json` and use the network.
    Outside the profile it writes `loom-live-profile: skip <test>: <why>` to the real stderr
    handle (libtest does not capture it); criterion output counts those lines and the
    `ignored, loom-live-profile:` lines into a `note: N tests need the live profile ...` line.
    A passing criterion shows it on a second line under the pass line (the criterion passes, so
    the note is the only sign those tests did not run); a failing or timed-out one shows it last,
    after the stdout and stderr tails; a cached pass shows none, because the cache keeps a 4 KiB
    output tail, too short to count from (revisit when a cache record stores the count). Inside the
    profile every skip (the guard's, `skip_unless`'s or `require_facility`'s) writes the same
    line and fails the entry, so an entry whose tests skipped never passes; a needed facility
    the platform's profile lacks writes an `unavailable` line and makes the entry unavailable.
    `codex` is a facility no environment provides yet, so plan 11's tests report unavailable
    inside the profile instead of passing on a skipped codex probe.
11. **The confined environment** drops `DISPLAY`, `WAYLAND_DISPLAY`, `XAUTHORITY` and
    `DBUS_SESSION_BUS_ADDRESS`; terminal spawners keep them. `XDG_RUNTIME_DIR` and the `TMUX*`
    names stay in the confined list (locations; bubblewrap's private `/tmp` hides the tmux
    socket on Linux). Revisit when a confined command is shown to reach a socket through either
    on macOS.
12. **`command_confinement: inherit` is gone**: the YAML value fails to parse; `confined` still
    parses (plans 04-11 carry it). Loom's own unit tests that must pass where bubblewrap cannot
    start use a `#[cfg(test)]`, never-deserialized `TestUnconfined` variant that no build of the
    binary contains. Deleting the single-variant plumbing is a follow-up.
13. **`allow_unsandboxed_escape` is gone** from plan and stage config; stage sessions always
    emit `"allowUnsandboxedCommands": false`.
14. **Codex trust surfaces.** A codex-licensed stage keeps writing `~/.codex` (codex keeps its
    SQLite state in `CODEX_SQLITE_HOME`, default `CODEX_HOME`, and replaces files atomically
    there, which a file-level bind mount breaks), but every capsule now also denies
    `~/.codex/rules/**`, `skills/**`, `plugins/**`, `packages/**` and `vendor_imports/**` beside
    the existing `hooks/**`, `hooks.json`, `config.toml`, `AGENTS.md`, `prompts/**` and
    `loom-skill-catalog/**`, so one stage cannot plant what the next stage's codex run loads.
    `auth.json` stays readable and writable to a codex-licensed stage (token refresh) and
    read-denied to every other stage.
15. **No knowledge-bootstrap.** The knowledge base maps this codebase; the distill stage records
    the profile and the removals.
16. **A merge resolver runs the live entries for the resolved tree.** `verify_resolved_completion`
    (`commands/stage/complete_resolved.rs`), which `--resolved` (`inbox_drain/merge_resolved.rs`)
    and a resolver's exit (`merge_handler/resolver_exit.rs`) both call, reaches the `live` check
    through `complete_verification::run_checks` against the stage worktree, the tree the
    resolver merged into (`merge_resolver_worktree`). The merge changed that tree's content, so
    the receipt bound before it stops counting. The merge signal of a stage with a
    `required: true` entry therefore tells the resolver to run `loom check --live <stage>` after
    committing the merge and before `loom stage merge <stage> --resolved`, and again after any
    later commit; with only advisory entries, or none, the signal says nothing and no run is
    needed. The request uses the stage session's relay (`HostJobRequest.live`, `JobKind::Live`):
    the relay matrix now applies a Merge session's `check`, and `inbox_drain/check.rs` admits it
    only with `live` and only from the resolver of that stage (the rule `inbox_drain/commit.rs`
    already applies to a resolver's commit). Nothing runs at `--resolved` or on
    `loom stage complete`. A resolver can neither dispute nor amend (`relay/matrix.rs`), and its
    `loom stage block` is a give-up that stops it and routes the stage to human review
    (`inbox_drain/apply.rs`), outside Decision 5's guard. It gives up at once only for an
    unavailable entry, which a person must remedy; a failed entry it fixes first when the merge
    broke it. The paths are in live-checks R2's brief.

## Execution Diagram

```mermaid
graph LR
    confinement-hardening --> live-gate-and-boundary
    test-escape-policy --> live-gate-and-boundary
    live-gate-and-boundary --> live-profile
    live-profile --> live-checks
    live-checks --> integration-verify
    integration-verify --> knowledge-distill
```

`confinement-hardening` & `test-escape-policy` run concurrently and share no file.

## Stages

### 1. confinement-hardening (`working_dir: loom`)

The environment split, the two removals, the codex trust surfaces, the live credential's place
among the state-root secrets, and the crates the gate needs (`cargo add` as the stage's
foundation). Stage necessity: Q1, `live-gate-and-boundary`'s contract session compiles a fake
upstream against `rcgen` and `rustls`, and a contract session cannot change `Cargo.toml`, so the
crates must be merged first; Q4, with test-escape-policy this is seven workers and three
cross-cutting removals, past 500,000 tokens with review rounds.

Risk walk: untrusted input (plan YAML naming `inherit` or `allow_unsandboxed_escape`:
`inherit-confinement-is-refused`, `unsandboxed-escape-field-is-gone`); configuration propagation
(the confined list reaches `confine.rs`: `criteria-prepare-uses-confined-environment`; terminals
keep theirs: `terminal-env-keeps-display-and-session-bus`); filesystem paths (codex trust
surfaces and the credential file: `codex-lane-denies-its-trust-surfaces`,
`every-capsule-denies-the-live-credential`). Behaviour: `confined-env-drops-display-and-session-bus`.

### 2. test-escape-policy (`working_dir: loom`)

`loom plan verify` refuses escape flags in plan-authored commands; the facility and
live-profile guards replace `LOOM_TEST_REQUIRE_SANDBOX_FREE` and `LOOM_E2E_REQUIRE_TMUX` across
the code, CI, scripts and docs; every operator test carries the live-profile guard. Stage
necessity: Q4 (see stage 1); it shares no file with stage 1 and needs nothing from later
stages.

Risk walk: untrusted input (escapes in nested `sh -c` and every command kind:
`plan-verify-refuses-escapes-in-every-command-kind`; prose is not a command:
`plan-verify-ignores-escape-words-in-prose`); configuration propagation (strictness follows the
advertised list, not one global switch: `facility-strictness-follows-the-advertised-list`);
filesystem paths (a symlinked or missing marker, a `HOME` equal to the account home:
`profile-check-needs-marker-home-and-env`); reachability (each operator test calls the guard:
wiring checks on the four files, plus `profile-tests-skip-outside-and-report-unavailable-inside`
for the decision itself).

### 3. live-gate-and-boundary (`working_dir: loom`)

The egress gate and the credential store; the live boundary inside the confined runner (the
platform builders split out of `confine_host.rs`, the live arguments and Seatbelt rules, the
runner's facility advertisement, the content digest). Stage necessity: Q3, the gate and the
boundary are the profile's security boundary, and their review must close before the profile
composes them (a gate or boundary defect found later invalidates the next stage's contracts);
Q4, two opus/xhigh workers with twenty contracts and security review rounds fill one session.

Risk walk: untrusted input (the proxy request head, credential bytes, the `Host` header:
`gate-refuses-connect-without-the-proxy-secret`, `credential-rejects-control-characters`,
`gate-refuses-host-header-mismatch`); filesystem paths (`credential-load-refuses-symlink`,
`credential-file-is-owner-only`); process I/O volume (`gate-streams-the-response-before-it-ends`,
`gate-stops-at-request-budget`); configuration propagation (the profile's environment and
`HOME` reach the command: `live-profile-environment-is-the-allowlist-plus-the-profile`,
`live-profile-home-is-private-and-the-account-home-is-not-writable`; the runner advertises what
it delivers: `runner-advertises-only-what-it-delivers`); lifecycle (content identity across a
commit: `content-digest-ignores-commit-identity`); security (`gate-refuses-unlisted-connect-host`,
`gate-loopback-listener-is-local-and-needs-the-secret`, `gate-injects-api-key`,
`gate-injects-oauth-bearer`, `gate-log-never-holds-secrets`,
`live-profile-reaches-the-gate-and-nothing-else`, `live-bwrap-args-add-home-gate-and-extras-only`);
external data (upstream verification and Seatbelt rule text:
`gate-refuses-untrusted-upstream`, `live-seatbelt-allows-only-the-gate-port`).

### 4. live-profile (`working_dir: loom`)

The run root, HOME seeding and one entry run end to end through the real boundary and gate;
the `loom live` commands (`exec`, `credential set|clear`, `status`, `smoke`); the `live:` field
through the schema, validation, the stage record, the check-definition hash, the job deadline
and the escape lint. Stage necessity: Q4 (three workers, seventeen contracts and their review
rounds; with the checks stage it would pass 500,000 tokens); Q3 for `live-checks`, which builds
receipts and completion on a profile whose end-to-end run is proven here.

Risk walk: untrusted input (stdin credential: `credential-set-reads-stdin-into-owner-only-file`;
plan YAML bounds: `live-entry-field-bounds`); lifecycle and authority
(`credential-set-refused-inside-a-session`, `entry-timeout-kills-the-command-and-forwarder`,
`live-exec-exits-with-the-command`); configuration propagation (`live-entry-defaults-reach-the-stage`,
`check-hash-binds-live-entries`, `deadline-counts-live-timeouts`,
`profile-marker-reaches-the-command`); security (`profile-home-carries-placeholders-never-the-credential`,
`plan-verify-scans-live-commands-for-escapes`); reachability (`profile-request-reaches-the-upstream-through-the-gate`,
`smoke-receipt-records-the-outcome`, `live-help-lists-subcommands`); external data (cargo's
zero-test output and the guard lines: `verdict-refuses-a-run-whose-tests-skipped`); version
gating (`live-requires-version-2`, `live-refused-on-knowledge-stages`).

### 5. live-checks (`working_dir: loom`)

`loom check --live` (relay, `JobKind::Live`, the host child, the operator's local run), the
receipt, the completion `live` check, the block rule, the "N tests need the live profile" note
(under a passing criterion's pass line and after a failing one's output tails),
`loom live status --stage`, the `--field live` dispute route, the merge resolver's live run
(Decision 16: the merge signal's live section and the relay's admission of a resolver's
`loom check --live`), and the plan-writer doctrine. Stage necessity: Q4 (four workers across
the host-job, completion, block and dispute paths with their review rounds).

Risk walk: configuration propagation (`--live` reaches a `Live` job, never a check:
`live-check-relays-as-a-live-job`); lifecycle (`tree-change-during-a-live-run-voids-the-binding`,
`live-receipt-binds-content-and-entry`, `resolution-voids-the-pre-merge-receipt`); authority
(`block-allowed-only-for-a-missing-credential`, `resolver-may-request-only-its-own-live-run`);
configuration propagation into the resolver's instructions
(`resolver-signal-asks-for-a-live-run-only-with-required-entries`);
external data (libtest's ignored-test line: `criterion-output-counts-profile-tests`; the note
reaching a passing criterion's output: `passing-criterion-output-carries-the-profile-note`); behaviour
(`completion-needs-a-current-passing-receipt-for-required-entries`,
`missing-credential-is-unavailable-and-runs-nothing`); reachability (the dispute route from
the CLI to the amendment: `live-dispute-field-is-accepted-by-the-cli`,
`accepted-live-amendment-rewrites-the-entry`; `reachable` checks from `run_job` and
`run_checks`).

### 6. integration-verify

Full Rust suite, lint, format and docs; review subagents (security of the gate, credential,
boundary, profile and escape lint; architecture of the live job, receipts, completion rule,
block rule and removals; test coverage against each contract's named wrong implementation); a
functional pass with the built binary; then one `loom stage block` naming
`bash ./dev-install.sh`, `loom live credential set` and `loom live smoke` for the one real API
call no stage can make. A retry completes only on a passing smoke receipt from the installed
binary.

### 7. knowledge-distill

Applies every stale-knowledge memory first, adds an `architecture/live-profile` topic, and a
"Live profile" section to `README.md`.

## Environment inventory

| Need | Stage(s) | State |
| --- | --- | --- |
| `crates.io`, `index.crates.io`, `static.crates.io` for `cargo add` (rcgen is not in the local registry) and builds | 1, every Rust stage | allowed (plan `allowed_domains`) |
| `web/node_modules` (repository provision lint; distill's markdownlint) | every stage | provisioned (`loom.provision`) |
| `registry.npmjs.org` (the repository's hooks) | every stage | allowed |
| Unix sockets and loopback in tests (gate, boundary, profile, CLI contracts) | 3, 4, 5, IV | criteria run in the host job, whose Linux runner permits both; a session's own run of those tests fails by design, so workers compile-check only and `loom check` is the gate |
| Nested bubblewrap in the boundary and profile contracts | 3, 4, IV | needs unprivileged user namespaces on the host (measured: works on the planning host under runner flags); a host without them fails those contracts loudly, never skips |
| `bwrap`, `git`, `sh`, `bash`, `jq` on the host for criteria and the hook test | all | present on the planning host |
| WebFetch for the external facts below | any | native tool, outside the Bash sandbox |
| A native Claude Code on the operator's `PATH` or in `~/.local/bin` | smoke (after IV's block) | operator prerequisite |
| A dedicated revocable credential: `claude setup-token` output, or a spend-capped Console API key | smoke, every live entry | operator prerequisite, piped into `loom live credential set --kind ...` from the repository root |
| Not needed by this plan | | KVM, a VMM, guest images, podman, root, socat (Claude Code's own sandbox needs socat once the follow-up plan's operator tests run planning sessions in the profile) |

Baseline: no gate was run at authoring (no test runs are allowed in this session, and plans
08-11 change the tree first). Every stage's build, lint, format, doc and maintainability lines
were green at the end of plan 07; the hook test `credential-guard-tokens.sh` runs in CI on every
push; the new contract targets are red at base by design.

## Unverified external facts

- Claude Code honours `HTTPS_PROXY` with userinfo (basic auth) and `NODE_EXTRA_CA_CERTS` when
  both come from `~/.claude/settings.json` `env` (its network docs, read 2026-10-28, say every
  network variable can be set there and that basic auth goes in the proxy URL). The smoke
  proves the pair end to end.
- A placeholder `CLAUDE_CODE_OAUTH_TOKEN` passes Claude Code's local checks and is sent as
  `authorization: Bearer` with its OAuth beta header; `apiKeyHelper` output is sent as
  `x-api-key` and `authorization` (the gate strips both and injects its own either way). The
  docs name `ANTHROPIC_AUTH_TOKEN` as Bearer and `ANTHROPIC_API_KEY` as `X-Api-Key`; the OAuth
  header is not documented.
- `hasCompletedOnboarding` plus loom's trust entry suffice for an interactive start in a fresh
  `HOME`; other first-run screens are unknown. Only the follow-up verification plan's required
  entry for plan 07's test proves the interactive path (the smoke uses `-p`).
- Seatbelt's `(remote ip "localhost:<port>")` rule text (srt's documentation says its macOS
  profile allows "only a specific localhost port"; the exact rule was not read) and the
  `sandbox_apply:` message of a refused nested Seatbelt. No macOS host is available.
- AF_UNIX connect through a read-only bind of the socket's directory (kernel: a read-only mount
  refuses write access only for regular files, directories and symlinks). Proven by contract
  `live-profile-reaches-the-gate-and-nothing-else`.
- reqwest's environment proxy sending `Proxy-Authorization` from the URL's userinfo (the profile
  contracts' probe client relies on it; the harness may set the header itself).
- libtest capturing only the print macros, so the guard's direct write to the stderr handle
  shows in an ordinary run (the counting is contract-tested; the capture behaviour is not).
- `claude -p --max-turns 1 --model haiku` as the smoke's minimal call.

## Open decisions

1. **`~/.codex/skills/**` and `rules/**` denied whole** (settled here as asked). The tree
   records that codex writes `skills/.system` and `rules/` itself. If codex refuses to start
   with them denied, the options are a dedicated `CODEX_HOME` for the codex lane seeded from the
   operator's config (its `auth.json` refresh then diverges from the operator's copy and may
   invalidate a rotating refresh token), or API-key auth for the lane (`CODEX_API_KEY`, a
   billing change).
2. **The narrower grant asked for (`~/.codex` not listed whole)** is not implemented: codex's
   root-level SQLite and atomic writes need the directory writable (Decision 14). The dedicated
   `CODEX_HOME` above is the route to it.
3. **`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` in the profile** also turns off feature-flag
   fetching, so a live test sees Claude Code without remote feature flags. Kept to bound the
   gate's traffic and budget; revisit if an operator test depends on a flagged feature.
4. **`hosts` and `credential` admit one value each** (`api.anthropic.com`, `anthropic`). They
   make an entry's egress visible in review; a codex credential kind (`api.openai.com`,
   `chatgpt.com`, codex's `auth.json` shape) is the expected second value.
5. **Implementation lanes** were not confirmed with the user (this plan was written by a
   subagent that cannot ask). Every stage plans on Claude workers; a codex lane can be added per
   stage if the user wants one.
6. **The block rule and the autonomous-recovery plan.** That plan sends every check with a
   dispute route to a dispute and lets the adjudicator block with a "needs-person" verdict. Its
   guard rewrite must keep `live::completion::block_refusal`'s rule or route the credential case
   to its needs-person verdict; its field enums must keep `Live`.
7. **Budget constants** (1000 requests, 1 GiB, 32 MiB) are not plan fields; revisit when an
   entry legitimately needs more.

## Residual risks

- A stage session controls the code a live entry runs, so it can spend the credential through
  `loom check --live`: the budget bounds one run (1000 requests, 1 GiB) and nothing bounds the
  number of runs, and every path on `api.anthropic.com` is reachable. The operator's control is
  a dedicated, revocable, spend-capped credential; the credential value itself never reaches
  the session.
- The macOS boundary is never executed while this plan runs: its Seatbelt rules are tested as
  text, and the run contracts execute on Linux. macOS's runner allows every `mach-lookup`, so a
  confined command (live or not) can reach the user's Keychain service; restricting it is a
  follow-up.
- On macOS the live profile provides neither `tmux` nor `bubblewrap`, so every operator test
  reports unavailable there; the profile serves `claude -p` style tests and the smoke.
- Claude Code's own sandbox inside the Linux profile needs bubblewrap and socat on the host;
  without socat, planning sessions (`failIfUnavailable: true`) fail inside the profile (a
  failure, not a skip).
- The forwarder's fixed `127.0.0.1:3128` inside the runner's namespace collides with a test that
  binds that port itself.
- A run root is removed when the entry ends, so a failing operator test leaves only its captured
  output and the gate log, not Claude Code's own debug files.
- The facility advertisement and the live profile take effect only after `dev-install.sh`: this
  plan's own stages run under the old installed daemon, so their contracts drive the in-tree
  boundary directly.
- Making a probe strict where its facility is advertised can turn a long-skipping test red
  after install (srt-based tests once the autonomous-recovery plan exposes srt's install root).
  That is the intended discovery; it lands on whichever plan runs next.
- An acceptance command that writes untracked, non-ignored files into the worktree voids a fresh
  live receipt. Completion runs acceptance before `run_checks` (`complete_pipeline.rs` runs
  `verify_acceptance` before `run_verification_phase`; `verify_resolved_completion` and the
  host completion job in `child_complete.rs` do the same), the `live` check sits in
  `run_checks`, and `content_digest` covers every tracked file and every untracked file git
  does not ignore. The digest `loom check --live` bound to then differs from the one the `live`
  check computes, so completion refuses with `run loom check --live <stage>` although the
  stage did not change since the run, in ordinary completion and after merge resolution alike.
  Criteria today write under `target/` (ignored) or the Cargo scratch (outside the tree), so
  the refusal does not occur. Revisit if a criterion writes tracked or unignored paths.

## Follow-ups

- **VM tier, minimal.** Only for tests that need root, kernel features or host services, and
  then the host's own userspace shared read-only with no disk images. Nothing else of the
  previous VM design (VMM runners, guest images, kernels, KVM prerequisites, snapshot disks) is
  carried forward.
- **A verification plan for the operator tests**, written after this plan is installed, with
  these entries (plan 07's and 08's on one stage that touches `tests/`, plan 09's and 11's on
  the stage that owns their files):

```yaml
live:
  - command: "cargo test --test plan_session_bridge_e2e operator_end_to_end_with_claude_code -- --ignored --exact"
    timeout_secs: 2400
    required: true
  - command: "cargo test --test planning_guidance_operator -- --ignored"
    timeout_secs: 3600
    required: true
  - command: "cargo test --test plan_chat_operator -- --ignored"
    timeout_secs: 1800
    required: true
  - command: "cargo test --test plan_handoff_operator -- --ignored"
    timeout_secs: 3600
    required: false
```

  Required where the Linux profile provides everything the test needs (Claude Code, tmux,
  bubblewrap, the gate); advisory for plan 11's tests, which also drive codex with its own
  credential and report unavailable until a codex credential kind exists.
- A codex credential kind and hosts for the gate.
- Delete the single-variant `CommandConfinement` plumbing.
- Restrict `mach-lookup` in the macOS runner profile.

## A loom defect found while planning

`plan/schema/validation/v2_lints/mod.rs` `stage_touches_dir` compares a stage's `files:` and
`artifacts:` entries, which are relative to the stage's `working_dir`, with `loom/src/cli`, a
repository-relative path (`v2_lints/loom_subcommands.rs`, `CLI_DIR`). A stage with
`working_dir: loom` that lists `src/cli/types.rs` therefore never counts as changing the CLI, and
a criterion calling the `loom` subcommand it adds is an error in v2 instead of a warning. This
plan avoids such criteria (its `loom live` checks are contracts and IV wiring); the fix belongs
to a later plan.

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
    - id: confinement-hardening
      name: "Confinement hardening"
      summary: "Confined commands lose the display and session-bus variables, the inherit confinement level and the unsandboxed-escape setting disappear, codex-licensed stages can no longer plant codex rules, skills or plugins for later stages, the live profile's credential file is hidden from every session and confined command, and the crates the gate needs are added."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: []
      description: |
        Build stage 1 of doc/plans/PLAN-live-test-lane.md (IN_PROGRESS-PLAN-live-test-lane.md once running). Read its Decisions 8 and 11-14 and doc/plans/briefs/live-test-lane/common.md first.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/; YAML paths are package-relative (../ reaches the repository root); brief paths are repo-relative. Plans 08-11 merged before this stage: locate every seam with loom map --outline / --find-all before briefing; line numbers in the briefs are advisory.
        PUBLIC SURFACE (the contract session writes the contracts from this text before any code):
        - loom::process::apply_confined_host_environment(command: &mut std::process::Command) and apply_confined_host_environment_from<I, K, V>(command: &mut Command, source: I) where I: IntoIterator<Item = (K, V)>, K: Into<OsString>, V: Into<OsString>: env_clear, then only the confined allowlist (every current stage name except DISPLAY, WAYLAND_DISPLAY, XAUTHORITY, DBUS_SESSION_BUS_ADDRESS).
        - loom::process::TERMINAL_ONLY_ENV_NAMES: &[&str] = ["DISPLAY", "WAYLAND_DISPLAY", "XAUTHORITY", "DBUS_SESSION_BUS_ADDRESS"]; loom::process::apply_stage_environment_from keeps its signature and still forwards those four (terminal spawners).
        - verify::criteria::confine (crate-internal): prepare_confined(spec: &CommandSpec, working_dir: Option<&Path>, tree: Option<&Path>, confinement: CommandConfinement) -> Result<PreparedCommand>; PreparedCommand::command(&self) -> &Command; with CommandConfinement::Confined the command's environment is the confined list.
        - CommandConfinement (loom::plan::schema / loom::models::stage) has no Inherit variant: plan YAML stage `sandbox: { command_confinement: inherit }` fails to parse with an error naming inherit; `command_confinement: confined` parses. A #[cfg(test)] #[serde(skip_deserializing)] TestUnconfined variant exists for loom's own unit tests only.
        - The field allow_unsandboxed_escape no longer exists on the plan-level sandbox config (loom::plan::schema::SandboxConfig) or on a stage's sandbox override: YAML naming it fails to parse as an unknown field.
        - crate::fs::permissions::state_root (crate-internal): LIVE_CREDENTIAL_FILE: &str = "live-credential", one of STATE_ROOT_SECRET_FILES (now 4 entries); token_deny_paths(resolved) therefore returns "/<resolved>/live-credential" beside the other secrets, search_ignore_body() holds a "live-credential" line and ripgrep_config_body() a "--glob=!live-credential" line.
        - sandbox::control_surfaces::session_denies(&DenyInputs { repo_root, state_root, worktree, executable_dirs, plugin_entries, writable_roots }) -> Result<SessionDenies { deny_write: Vec<String>, edit: Vec<String> }> (crate-internal; partition_admin_dirs reads the repository's git dir, so a fixture needs `git init`): deny_write holds "~/.codex/rules", "~/.codex/skills", "~/.codex/plugins", "~/.codex/packages", "~/.codex/vendor_imports" and "~/.codex/AGENTS.md", edit holds "Edit(~/.codex/rules/**)", and neither denies "~/.codex", "~/.codex/auth.json" or "~/.codex/sessions".
        CONTRACT FILES: tests/hardening_contracts.rs (integration; parse plan YAML with serde_saphyr::from_str::<loom::plan::schema::LoomMetadata>), src/verify/criteria/confine_env_contract_tests.rs, src/fs/permissions/live_credential_contract_tests.rs, src/sandbox/control_surfaces/codex_trust_tests.rs (loom derives the in-crate scaffold). Tests that set process environment variables are #[serial_test::serial] and restore them.
        FOUNDATION (you, before any worker): (1) the crates, each with cargo add from loom/: cargo add tokio@1.53 --features rt,net,io-util,time,sync,macros; cargo add tokio-rustls@0.26; cargo add rustls@0.23; cargo add hyper@1 --features http1,server,client; cargo add hyper-util@0.1 --features tokio; cargo add http-body-util@0.1; cargo add rustls-platform-verifier@0.7; cargo add rcgen@0.14 --no-default-features --features aws_lc_rs,pem; cargo add base64@0.22. Every one but rcgen is already in Cargo.lock through reqwest; they stay unused until stage 3 (no unused-dependency check runs in this repository). (2) src/models/stage/sandbox_config.rs: replace the Inherit variant with the #[cfg(test)] #[serde(skip_deserializing)] TestUnconfined variant (doc text in the H2a brief) and delete the stage override field allow_unsandboxed_escape with its doc. Then cargo build --all-targets lists the consumers H2a and H2b own.
        WAVES: (1) foundation. (2) H1, H2b, H3 in parallel. (3) H2a (it calls H1's function).
        Territories are DISJOINT. Workers NEVER spawn subagents. Spawn every worker BY AGENT TYPE, all of one wave in ONE message, each with a short fixed prompt plus "Your brief: <path>. Read it in full before anything else." loom-software-engineer rows run at sonnet; the opus row uses loom-senior-software-engineer.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | H1 | Environment split, live credential secret, policy revision | sonnet | src/process/environment.rs; src/process/mod.rs; src/fs/permissions/state_root.rs; src/handoff/completion/identity.rs; src/plan_session/sandbox/tests.rs; src/plan_session/sandbox/tests_kinds_fixture.rs; ../loom-hooks/credential-guard.sh; ../loom-hooks/tests/credential-guard-tokens.sh | common.md | doc/plans/briefs/live-test-lane/confinement-hardening/h1-env-split.md |
        | H2a | Remove the inherit level | sonnet | src/verify/criteria/confine.rs; src/verify/criteria/cache_fingerprint.rs; src/verify/criteria/runner.rs; src/verify/criteria/confine_host_tests.rs; src/verify/criteria/confine_host_tool_tests.rs; src/verify/criteria/confine_cargo_home_tests.rs; src/verify/criteria/executor_lifecycle_tests.rs; src/verify/criteria/tests/confine_tests.rs; src/verify/criteria/tests/runner_tests.rs; src/verify/criteria/tests/cache_tests.rs; src/verify/criteria/tests/cache_contract_tests.rs; src/verify/criteria/tests/criterion_timeout_tests.rs; src/verify/goal_backward/truths.rs; src/models/stage/methods_tests.rs; src/plan/schema/validation/v2_lints/registry_domains.rs; src/plan/schema/tests/v2_lint_environment_tests.rs; tests/plan_environment_contracts.rs | common.md | doc/plans/briefs/live-test-lane/confinement-hardening/h2a-inherit.md |
        | H2b | Remove the unsandboxed escape | sonnet | src/sandbox/config.rs; src/sandbox/config/preflight.rs; src/sandbox/config/preflight/tests.rs; src/sandbox/settings/policy.rs; src/sandbox/settings/policy/tests.rs; src/sandbox/settings/tests.rs; src/sandbox/settings/tests_read_denies.rs; src/sandbox/grant_paths.rs; src/orchestrator/terminal/native/build_cache.rs; src/plan/schema/types.rs; tests/integration/plan_verify.rs | common.md | doc/plans/briefs/live-test-lane/confinement-hardening/h2b-unsandboxed-escape.md |
        | H3 | Codex trust surfaces | opus | src/sandbox/control_surfaces.rs; src/sandbox/control_surfaces/tests.rs; src/sandbox/control_surfaces/tests_home_surfaces.rs; src/sandbox/control_surfaces/tests_session_denies.rs; src/sandbox/control_surfaces/session_denies.rs; src/codex.rs | common.md | doc/plans/briefs/live-test-lane/confinement-hardening/h3-codex-surfaces.md |

        A struct literal or pinned count the compiler or a test names in a file no row owns goes to the row whose change caused it; record the file in loom memory.
        INTEGRITY: expected events, filed together in ONE loom stage dispute-integrity confinement-hardening after the final review (copy the ids from loom stage review integrity confinement-hardening): TI-edit on src/sandbox/control_surfaces/tests_home_surfaces.rs (rules and skills are denied now), tests/plan_environment_contracts.rs (inherit is a parse error now), and any H1 expected-list file that raises one; TI-decl-rust and TI-assert-rust for the tests whose subject no longer exists (H2a and H2b report each). Each reason names the decision in the plan.
        AFTER THE WAVES: cargo fmt --all; one loom check confinement-hardening; the review rounds (one reviewer briefed with Decisions 8 and 11-14 and the security-audit skill: every capsule kind, Linux and macOS emission, no remaining route to an unconfined plan command, the hook's new case). No maintainability-baseline.txt row changes.
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too), including stale-knowledge notes: concerns/sandbox-and-confinement-gaps.md "The Codex Lane Grants All of ~/.codex" says AGENTS.md is writable while HOME_SURFACES denies it; NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "Cargo.toml"
        - "Cargo.lock"
        - "src/process/environment.rs"
        - "src/process/mod.rs"
        - "src/fs/permissions/state_root.rs"
        - "src/fs/permissions/mod.rs"
        - "src/fs/permissions/live_credential_contract_tests.rs"
        - "src/handoff/completion/identity.rs"
        - "src/plan_session/sandbox/tests.rs"
        - "src/plan_session/sandbox/tests_kinds_fixture.rs"
        - "src/orchestrator/terminal/native/build_cache.rs"
        - "src/models/stage/sandbox_config.rs"
        - "src/models/stage/methods_tests.rs"
        - "src/verify/criteria/**"
        - "src/verify/goal_backward/truths.rs"
        - "src/plan/schema/types.rs"
        - "src/plan/schema/validation/v2_lints/registry_domains.rs"
        - "src/plan/schema/tests/v2_lint_environment_tests.rs"
        - "src/sandbox/**"
        - "src/codex.rs"
        - "tests/hardening_contracts.rs"
        - "tests/hardening_contracts/**"
        - "tests/plan_environment_contracts.rs"
        - "tests/integration/plan_verify.rs"
        - "../loom-hooks/credential-guard.sh"
        - "../loom-hooks/tests/credential-guard-tokens.sh"
      acceptance:
        - "cargo build"
        - "cargo test --lib process::environment::"
        - "cargo test --lib verify::criteria::"
        - "cargo test --lib sandbox::"
        - "cargo test --lib fs::permissions::"
        - "cargo test --lib plan_session::sandbox::"
        - "cargo test --lib models::stage::"
        - "cargo test --lib plan::schema::"
        - "cargo test --lib handoff::completion::"
        - "cargo test --test hardening_contracts"
        - "cargo test --test plan_environment_contracts"
        - "cargo test --test maintainability"
        - "bash ../loom-hooks/tests/credential-guard-tokens.sh"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
      before_stage:
        - command: "cargo --version"
          description: "cargo on PATH"
        - command: "test -d src/plan_server/summaries"
          description: "plan 10 (summary rebuilds) is merged"
        - command: "test -f tests/plan_handoff_operator.rs"
          description: "plan 11 (execution handoff) is merged"
      after_stage:
        - command: "rg -qF '\"live-credential\"' src/fs/permissions/state_root.rs"
          description: "the live credential is one of the state-root secrets"
      artifacts:
        - "src/process/environment.rs"
        - "src/sandbox/control_surfaces.rs"
      wiring:
        - source: "src/verify/criteria/confine.rs"
          pattern: "apply_confined_host_environment("
          literal: true
          description: "confined criteria take the confined host allowlist"
        - source: "src/orchestrator/terminal/native/spawner.rs"
          pattern: "apply_stage_environment("
          literal: true
          description: "the terminal spawner keeps the terminal allowlist"
        - source: "src/fs/permissions/state_root.rs"
          pattern: "LIVE_CREDENTIAL_FILE,"
          literal: true
          description: "the secret list every capsule derives from names the live credential"
      harness: ["tests/hardening_contracts/**"]
      contracts:
        - id: confined-env-drops-display-and-session-bus
          file: tests/hardening_contracts.rs
          test: confined_env_drops_display_and_session_bus
          scenario: "apply_confined_host_environment_from on Command::new(\"env\") with HOME, PATH, CARGO_HOME, DISPLAY, WAYLAND_DISPLAY, XAUTHORITY and DBUS_SESSION_BUS_ADDRESS in the source, then reads command.get_envs()"
          rejects: "a split that leaves the display and session-bus names in the list confined commands get, instead of HOME, PATH and CARGO_HOME present and all four absent"
        - id: terminal-env-keeps-display-and-session-bus
          file: tests/hardening_contracts.rs
          test: terminal_env_keeps_display_and_session_bus
          scenario: "apply_stage_environment_from with the same source; TERMINAL_ONLY_ENV_NAMES compared with the four names"
          rejects: "deleting the four names from the shared list, which breaks terminal windows, instead of DISPLAY and DBUS_SESSION_BUS_ADDRESS still forwarded to terminal spawners"
        - id: inherit-confinement-is-refused
          file: tests/hardening_contracts.rs
          test: inherit_confinement_is_refused
          scenario: "parses a minimal v2 LoomMetadata YAML whose stage sandbox sets command_confinement: inherit, then the same YAML with confined"
          rejects: "a schema that still accepts inherit and runs that stage's criteria with the ambient environment, instead of an error naming inherit and a successful parse of confined"
        - id: unsandboxed-escape-field-is-gone
          file: tests/hardening_contracts.rs
          test: unsandboxed_escape_field_is_gone
          scenario: "parses a v2 LoomMetadata YAML with allow_unsandboxed_escape: false in a stage sandbox override, then one with it in the plan-level sandbox block"
          rejects: "keeping the field (refused only when true at preflight), instead of an unknown-field parse error for both"
        - id: criteria-prepare-uses-confined-environment
          file: src/verify/criteria/confine_env_contract_tests.rs
          test: verify::criteria::confine_env_contract_tests::confined_command_omits_session_bus_and_display
          scenario: "with DBUS_SESSION_BUS_ADDRESS and DISPLAY set in the process environment (serial, restored after), prepare_confined(&CommandSpec::shell(\"true\"), Some(tempdir), None, CommandConfinement::Confined) and reads prepared.command().get_envs()"
          rejects: "confine.rs still calling apply_stage_environment, so criteria keep the session bus, instead of both names absent and PATH present"
        - id: every-capsule-denies-the-live-credential
          file: src/fs/permissions/live_credential_contract_tests.rs
          test: fs::permissions::live_credential_contract_tests::every_capsule_denies_the_live_credential
          scenario: "token_deny_paths(\"/repo/.loom/work\"), search_ignore_body() and ripgrep_config_body(), and STATE_ROOT_SECRET_FILES compared with LIVE_CREDENTIAL_FILE"
          rejects: "a credential denied in one capsule writer only, or through a home-relative ~/.loom path that a relocated LOOM_HOME escapes, instead of live-credential in the one state-root secret list that every capsule, the search exclusions and the confined runner derive from"
        - id: codex-lane-denies-its-trust-surfaces
          file: src/sandbox/control_surfaces/codex_trust_tests.rs
          test: sandbox::control_surfaces::codex_trust_tests::codex_lane_denies_its_trust_surfaces
          scenario: "session_denies for a git-initialised TempDir repository with worktree None, no executable dirs, plugin_entries Some(&[]) and no writable roots"
          rejects: "a capsule that still grants ~/.codex with no deny under it for rules/, skills/ and plugins/ (so one stage plants what the next stage's codex loads), or one that denies ~/.codex or auth.json whole, instead of each trust surface in deny_write and Edit(~/.codex/rules/**) while ~/.codex, auth.json and sessions stay writable"

    - id: test-escape-policy
      name: "Test escape policy"
      summary: "loom plan verify refuses escape flags in plan-authored commands, tests that drive a real Claude Code run only inside the live profile, and probes for facilities an environment claims to provide fail instead of skipping, replacing the old host-run switches in code, CI, scripts and docs."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "loom"
      dependencies: []
      description: |
        Build stage 2 of doc/plans/PLAN-live-test-lane.md (IN_PROGRESS- once running). Read its Decision 10 and doc/plans/briefs/live-test-lane/common.md ("Test guards") first.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/; YAML paths are package-relative (../ reaches the repository root); brief paths are repo-relative. Plans 08-11 merged: confirm every operator test name with loom map --outline before briefing.
        PUBLIC SURFACE (the contract session writes the contracts from this text before any code):
        - loom::process::sandbox_probe (items in its child module facilities, re-exported): FACILITIES_ENV: &str = "LOOM_TEST_FACILITIES"; enum Facility { UnixSockets, Loopback, ProcessTree, Bubblewrap, Tmux, NetworkGate, WritableHome, ClaudeCode, Codex } (Debug, Clone, Copy, PartialEq, Eq, Hash) with name() -> &'static str ("unix-sockets", "loopback", "process-tree", "bubblewrap", "tmux", "network-gate", "writable-home", "claude-code", "codex"), from_name(&str) -> Option<Facility> and ALL: [Facility; 9]; facilities_from(value: Option<&str>) -> Vec<Facility> (comma separated, trimmed, unknown names and duplicates dropped, ALL order); advertised_facilities() -> Vec<Facility>.
        - enum FacilityDecision { Proceed, Skip, Fail }; facility_decision(facility: Facility, probe_ok: bool, advertised: &[Facility]) -> FacilityDecision (Proceed when probe_ok; else Fail when advertised contains facility; else Skip); require_facility(facility, probe_ok, test_name: &str, why: &str) -> bool (true means skip; panics on Fail with a message naming test_name, why and the facility name).
        - PROFILE_ENV: &str = "LOOM_LIVE_PROFILE"; PROFILE_MARKER: &str = ".loom-live-profile"; PROFILE_LINE_PREFIX: &str = "loom-live-profile:"; account_home() -> Option<PathBuf> (the euid's passwd home, never $HOME); check_profile(home: Option<&Path>, run_id: Option<&str>, account_home: Option<&Path>) -> Result<(), String>: Ok only when run_id is non-empty, home is absolute, home/.loom-live-profile is a regular file (not a symlink) whose trimmed content equals run_id, account_home is known and home differs from it after canonicalize; in_live_profile() -> bool over HOME, LOOM_LIVE_PROFILE and account_home().
        - enum ProfileDecision { Run, Skip(String), Unavailable(String) }; live_profile_decision(test_name: &str, needs: &[Facility], profile: Result<(), String>, advertised: &[Facility]) -> ProfileDecision: Err(why) gives Skip("loom-live-profile: skip <test_name>: <why>"); else the first need not in advertised gives Unavailable("loom-live-profile: unavailable <test_name>: needs <name>, which this platform's live profile does not provide"); else Run. require_live_profile(test_name, needs) -> bool writes the Skip or Unavailable line to the process's stderr handle and returns true only for Run.
        - sandbox_probe::skip_unless(probe_ok, test_name, why) -> bool stays and never panics; inside the live profile (in_live_profile()) every skip, from skip_unless or require_facility, also writes "loom-live-profile: skip <test_name>: <why>" to the stderr handle, so a live run whose tests skipped fails. sandbox_probe::bubblewrap_blocker() -> Result<(), String> (memoized).
        - loom plan verify (the built binary, env!("CARGO_BIN_EXE_loom") plan verify --strict <plan>) reports an error for each plan-authored command (acceptance, setup, wiring_tests, before_stage, after_stage, dead_code_check, provision; nested sh -c scripts included) that uses --include-ignored, --dangerously-bypass-approvals-and-sandbox, --yolo, --dangerously-skip-permissions, --sandbox danger-full-access (also --sandbox=danger-full-access and -s danger-full-access), LOOM_TEST_REQUIRE_SANDBOX_FREE, LOOM_LIVE_PROFILE or dangerouslyDisableSandbox; each error names the escape and the command label (Acceptance criterion #n, Setup command #n, Wiring test #n '<name>', before_stage check #n, after_stage check #n, provision entry #n). Stage description, summary and name text is never scanned; --ignored is not an escape.
        CONTRACT FILES: tests/escape_lint_contracts.rs (a temp git repository holding a complete valid v2 PLAN.md: bookends, one standard stage with one contract; mirror the plan-building helpers of tests/plan_environment_contracts.rs in tests/escape_lint_contracts/support.rs) and tests/test_guard_contracts.rs (pure functions with TempDir paths; no test sets HOME).
        WAVES: (1) X1 and H4 in parallel, ONE message. (2) H5 (it converts call sites to H4's API). Territories are DISJOINT. Workers NEVER spawn subagents. Spawn BY AGENT TYPE (loom-software-engineer, sonnet) with the fixed prompt plus "Your brief: <path>. Read it in full before anything else."

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | X1 | Escape lint | sonnet | src/plan/schema/validation/v2_lints/escape_hatches.rs; src/plan/schema/validation/v2_lints/mod.rs; src/plan/schema/tests/v2_lint_escape_tests.rs; src/plan/schema/tests/mod.rs; src/plan/schema/tests/validation_suite_tests.rs | common.md | doc/plans/briefs/live-test-lane/test-escape-policy/x1-escape-lint.md |
        | H4 | Facility and profile guards, operator tests | sonnet | src/process/sandbox_probe.rs; src/process/sandbox_probe/facilities.rs; tests/plan_session_bridge_e2e/operator.rs; tests/planning_guidance_operator.rs; tests/plan_chat_operator.rs; tests/plan_handoff_operator.rs | common.md | doc/plans/briefs/live-test-lane/test-escape-policy/h4-profile-guard.md |
        | H5 | Probe call sites, CI, scripts and docs | sonnet | tests/plan_session_confinement.rs; tests/plan_session_confinement/srt.rs; src/orchestrator/terminal/native/tests_capsule_bwrap.rs; tests/e2e/tmux_backend.rs; tests/e2e/web_terminal.rs; tests/host_verification_e2e.rs; tests/host_verification_contracts.rs; tests/plan_session_bridge_e2e.rs; tests/integration/hooks_poll_guard.rs; tests/integration/hooks_subagent_verify_guard.rs; tests/integration/hooks_spawn_guard.rs; src/plan_server/ws/tests.rs; src/process/tests.rs; src/context/refresh/tests_source_graph/enumeration.rs; ../.github/workflows/ci.yml; ../scripts/rust_test_environment.py; ../doc/testing.md; ../doc/sandbox-release-checklist.md; ../CONTRIBUTING.md | common.md | doc/plans/briefs/live-test-lane/test-escape-policy/h5-facility-sweep.md |

        AFTER THE WAVES: cargo fmt --all; one loom check test-escape-policy; the review rounds (one reviewer on evasion of the lint: quoting, nested scripts, provision, and false positives in prose; one on the guards: can an exported variable alone run an operator test against a real HOME, does any converted probe now fail where the runner does not provide the facility). No maintainability-baseline.txt row changes. If integrity events arise (the skip_unless unit test's assertion text, a converted helper), dispute them in ONE filing with the reason "strictness follows the advertised facilities (Decision 10)".
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too), including stale-knowledge notes for every knowledge section that tells a reader to set LOOM_TEST_REQUIRE_SANDBOX_FREE or LOOM_E2E_REQUIRE_TMUX (execution-containment.md "Confinement E2E Lives Outside the Sandbox", planning-session-sandbox.md "Confinement test", planning-session-bridge.md, sandbox-and-confinement-gaps.md "Sandbox Denial Has No End-to-End CI Canary"); NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "src/plan/schema/validation/v2_lints/escape_hatches.rs"
        - "src/plan/schema/validation/v2_lints/mod.rs"
        - "src/plan/schema/tests/v2_lint_escape_tests.rs"
        - "src/plan/schema/tests/mod.rs"
        - "src/plan/schema/tests/validation_suite_tests.rs"
        - "src/process/sandbox_probe.rs"
        - "src/process/sandbox_probe/**"
        - "tests/plan_session_bridge_e2e/operator.rs"
        - "tests/planning_guidance_operator.rs"
        - "tests/plan_chat_operator.rs"
        - "tests/plan_handoff_operator.rs"
        - "tests/plan_session_confinement.rs"
        - "tests/plan_session_confinement/srt.rs"
        - "src/orchestrator/terminal/native/tests_capsule_bwrap.rs"
        - "tests/e2e/tmux_backend.rs"
        - "tests/e2e/web_terminal.rs"
        - "tests/host_verification_e2e.rs"
        - "tests/host_verification_contracts.rs"
        - "tests/plan_session_bridge_e2e.rs"
        - "tests/integration/hooks_poll_guard.rs"
        - "tests/integration/hooks_subagent_verify_guard.rs"
        - "tests/integration/hooks_spawn_guard.rs"
        - "src/plan_server/ws/tests.rs"
        - "src/process/tests.rs"
        - "src/context/refresh/tests_source_graph/enumeration.rs"
        - "tests/escape_lint_contracts.rs"
        - "tests/escape_lint_contracts/**"
        - "tests/test_guard_contracts.rs"
        - "tests/test_guard_contracts/**"
        - "../.github/workflows/ci.yml"
        - "../scripts/rust_test_environment.py"
        - "../doc/testing.md"
        - "../doc/sandbox-release-checklist.md"
        - "../CONTRIBUTING.md"
      acceptance:
        - "cargo build"
        - "cargo test --lib plan::schema::"
        - "cargo test --lib process::"
        - "cargo test --lib plan_server::ws::"
        - "cargo test --test escape_lint_contracts"
        - "cargo test --test test_guard_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - 'python3 -I -c "import ast, sys; ast.parse(open(sys.argv[1]).read())" ../scripts/rust_test_environment.py'
      before_stage:
        - command: "test -f tests/plan_session_bridge_e2e/operator.rs"
          description: "plan 07's operator end-to-end test exists"
        - command: "rg -q 'fn operator' tests/planning_guidance_operator.rs"
          description: "plan 08's guidance operator tests share fn operator"
        - command: "test -f tests/plan_chat_operator.rs"
          description: "plan 09's chat operator test exists"
        - command: "test -f tests/plan_handoff_operator.rs"
          description: "plan 11's handoff operator tests exist"
      after_stage:
        - command: "rg -q 'pub fn require_live_profile' src/process/sandbox_probe/facilities.rs"
          description: "the live-profile guard exists for the profile stages"
      artifacts:
        - "src/plan/schema/validation/v2_lints/escape_hatches.rs"
        - "src/process/sandbox_probe/facilities.rs"
      wiring:
        - source: "src/plan/schema/validation/v2_lints/mod.rs"
          pattern: "escape_hatches::check("
          literal: true
          description: "plan verify runs the escape lint"
        - source: "tests/plan_session_bridge_e2e/operator.rs"
          pattern: "require_live_profile("
          literal: true
          description: "plan 07's operator test runs only in the live profile"
        - source: "tests/planning_guidance_operator.rs"
          pattern: "require_live_profile("
          literal: true
          description: "plan 08's operator tests run only in the live profile"
        - source: "tests/plan_chat_operator.rs"
          pattern: "require_live_profile("
          literal: true
          description: "plan 09's operator test runs only in the live profile"
        - source: "tests/plan_handoff_operator.rs"
          pattern: "require_live_profile("
          literal: true
          description: "plan 11's operator tests run only in the live profile"
        - source: "tests/e2e/tmux_backend.rs"
          pattern: "Facility::Tmux"
          literal: true
          description: "the tmux e2e probe follows the advertised facilities"
      harness: ["tests/escape_lint_contracts/**", "tests/test_guard_contracts/**"]
      contracts:
        - id: plan-verify-refuses-escapes-in-every-command-kind
          file: tests/escape_lint_contracts.rs
          test: plan_verify_refuses_escapes_in_every_command_kind
          scenario: "a valid v2 plan whose standard stage has acceptance cargo test -- --include-ignored, setup LOOM_TEST_REQUIRE_SANDBOX_FREE=1 true, a wiring test codex exec --sandbox danger-full-access x, a before_stage check LOOM_LIVE_PROFILE=r1 cargo test, an after_stage check sh -c 'claude --dangerously-skip-permissions -p x', and a provision entry codex --yolo; runs loom plan verify --strict on it"
          rejects: "a lint that scans acceptance only, or misses nested sh -c scripts, before_stage checks or provision entries, instead of a non-zero exit whose output names all six escapes with their command labels"
        - id: plan-verify-ignores-escape-words-in-prose
          file: tests/escape_lint_contracts.rs
          test: plan_verify_ignores_escape_words_in_prose
          scenario: "the same valid plan with every escape word only in a stage description and summary, and an acceptance cargo test -- --ignored"
          rejects: "a lint that greps the plan file text, or treats --ignored as --include-ignored, instead of no escape error in the output"
        - id: facility-strictness-follows-the-advertised-list
          file: tests/test_guard_contracts.rs
          test: facility_strictness_follows_the_advertised_list
          scenario: "facilities_from(Some(\" tmux,bubblewrap,nonsense,tmux \")); facility_decision(Tmux, false, that list), (Loopback, false, that list), (Tmux, true, that list)"
          rejects: "one global strict switch (every probe strict or none) or a parser that keeps unknown names, instead of [Bubblewrap, Tmux] and Fail, Skip, Proceed"
        - id: profile-check-needs-marker-home-and-env
          file: tests/test_guard_contracts.rs
          test: profile_check_needs_marker_home_and_env
          scenario: "check_profile with TempDir homes: run id None; marker missing; marker a symlink to a file holding the id; marker holding another id; home equal to account_home; and a home with a regular marker holding the id and a different account_home"
          rejects: "an environment-only check (any shell can export LOOM_LIVE_PROFILE) or one that accepts the account's own home, instead of Err for the first five and Ok for the last"
        - id: profile-tests-skip-outside-and-report-unavailable-inside
          file: tests/test_guard_contracts.rs
          test: profile_tests_skip_outside_and_report_unavailable_inside
          scenario: "live_profile_decision(\"operator_x\", &[ClaudeCode, Tmux], Err(\"no marker\"), &[]), then with Ok(()) and advertised [ClaudeCode], then with Ok(()) and advertised [ClaudeCode, Tmux]"
          rejects: "a guard that panics outside the profile, runs inside it whatever is missing, or skips without a parseable line, instead of Skip starting loom-live-profile: skip operator_x:, Unavailable naming tmux, and Run"

    - id: live-gate-and-boundary
      name: "Live gate and boundary"
      summary: "Adds the host-side egress gate and the per-project live credential, and teaches loom's confined runner the live boundary: a private home, one path to the gate, and a list of the facilities it provides."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: ["confinement-hardening", "test-escape-policy"]
      description: |
        Build stage 3 of doc/plans/PLAN-live-test-lane.md (IN_PROGRESS- once running). Read its "The runner, read from the code", Decisions 2, 6, 8 and 10, and doc/plans/briefs/live-test-lane/common.md first.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/; YAML paths are package-relative; brief paths are repo-relative.
        PUBLIC SURFACE (the contract session writes the contracts from this text before any code):
        - loom::live::credential: CredentialKind { ApiKey, OauthToken } (serde and FromStr api-key | oauth-token); Credential::new(kind, raw: &str) -> Result<Credential> (strips one trailing \n or \r\n; refuses empty, more than 4096 bytes, any byte outside 0x21..=0x7e); Credential::kind(); Credential::expose(&self) -> &str; credential_path(work_dir: &Path) -> PathBuf (<work_dir>/live-credential); store(work_dir, &Credential) -> Result<()> (mode 0600); load(work_dir) -> Result<Option<Credential>> (Err for a symlink, a non-regular file, a mode with group or other bits, another owner, more than 8 KiB); clear(work_dir) -> Result<bool>.
        - loom::live::gate: ANTHROPIC_HOSTS: &[&str] = ["api.anthropic.com"]; PROXY_USER: &str = "loom-live"; Budget { max_requests: u32, max_response_bytes: u64, max_request_bytes: u64 } (Default 1000 / 1 GiB / 32 MiB); Listen { Unix(PathBuf), Loopback }; GateConfig { listen: Listen, hosts: Vec<String>, credential: Credential, budget: Budget, upstream: Upstream }; start(GateConfig) -> Result<GateHandle>; GateHandle::ca_pem(&self) -> &str (the per-run CA, PEM); GateHandle::proxy_secret(&self) -> &str; GateHandle::endpoint(&self) -> &Endpoint; GateHandle::proxy_url(&self, host_port: &str) -> String ("http://loom-live:<secret>@<host_port>"); GateHandle::stop(self) -> GateReport { requests: u32, refused: u32, budget_exhausted: bool, response_bytes: u64, log: Vec<String> }; Endpoint { Unix(PathBuf), Loopback(SocketAddr) } (Loopback always 127.0.0.1); Upstream::public() -> Result<Upstream>; Upstream::fixed(addr: SocketAddr, roots: rustls::RootCertStore) -> Upstream (every host connects to addr and verifies with roots, SNI = the CONNECT host).
        - Gate protocol: a client connects to the endpoint and sends CONNECT <host>:443 HTTP/1.1 with Proxy-Authorization: Basic base64(loom-live:<secret>). Missing or wrong secret: 407 and no upstream contact; a host outside hosts or a port other than 443: 403 and no upstream contact; any other method: 405. After 200 the client speaks TLS with SNI <host> trusting ca_pem, ALPN http/1.1. Per request the gate answers 421 when Host differs from the CONNECT host, 429 once max_requests is spent (no upstream contact), 502 when the upstream certificate does not verify; strips authorization, x-api-key and proxy-authorization and injects x-api-key: <value> (ApiKey) or authorization: Bearer <value> (OauthToken), passing anthropic-beta through; streams the response body (a chunk reaches the client before the upstream ends the body). Log lines hold "<seq> <METHOD> <host><path-without-query> <status> <bytes>" and refused connections, never headers or bodies.
        - loom::verify::criteria: LiveBoundary { home: PathBuf, tmp: PathBuf, gate: GateMount, read_only: Vec<PathBuf>, env: Vec<(OsString, OsString)>, facilities: Vec<loom::process::sandbox_probe::Facility> }; GateMount { UnixDir(PathBuf), LoopbackPort(u16) }; live_bwrap_args(&LiveBoundary) -> Vec<OsString> (exactly "--bind <home> <home>", then "--ro-bind <dir> <dir>" for UnixDir, then "--ro-bind <p> <p>" per read_only); live_seatbelt_rules(&LiveBoundary) -> String (file-read and file-write for home and tmp, a literal read rule for a file in read_only and a subpath rule for a directory, exactly one (allow network-outbound (remote ip "localhost:<port>")) for LoopbackPort, no other network rule); run_live_with_timeout(spec: &CommandSpec, tree: &Path, working_dir: &Path, timeout: Duration, boundary: &LiveBoundary) -> Result<CriterionResult> (the standard confined runner, network denied, plus the live arguments or rules; HOME = boundary.home, RUSTUP_HOME pinned to the original, PATH led by <home>/.local/bin, then boundary.env; LOOM_TEST_FACILITIES = runner_facilities plus boundary.facilities); runner_facilities(path: Option<&OsStr>) -> Vec<Facility> (Linux: UnixSockets, Loopback, ProcessTree, Bubblewrap when bubblewrap_blocker() is Ok, Tmux when tmux resolves on path; elsewhere empty). Every standard confined command carries LOOM_TEST_FACILITIES = runner_facilities. Crate-internal: content_digest(checkout: &Path) -> Option<String> (equal for a dirty tree and the commit of the same bytes); zero_test_failure becomes pub(crate); confine_host::wrap(original, tree, network, live: Option<&LiveBoundary>), whose eight callers (confine.rs prepare and seven test call sites) pass the new argument; platform_command (all three platform variants) takes live: Option<&LiveBoundary> as an eighth, trailing parameter under #[allow(clippy::too_many_arguments)] with a // reason line in the form of src/orchestrator/terminal/native/wrapper.rs lines 65-67, so its two callers (wrap and the confine_host_tests.rs call) append one argument and keep the other seven.
        CONTRACT FILES: tests/live_gate_contracts.rs with harness tests/live_gate_contracts/** (a fake upstream: a rustls server over a std TcpListener on 127.0.0.1 with a test CA made by rcgen, recording each request's headers and able to hold a response mid-body; a blocking client over std UnixStream or TcpStream that sends CONNECT and then speaks TLS with rustls; use rustls::crypto::aws_lc_rs::default_provider() and the builder_with_provider constructors, never ServerConfig::builder()). src/verify/criteria/confine_live_contract_tests.rs with harness src/verify/criteria/confine_live_contract_tests/** (loom derives the scaffold line in src/verify/criteria/mod.rs). Its run contracts execute probes inside the boundary by re-running the lib test binary (std::env::current_exe(), added to boundary.read_only) with --ignored --exact verify::criteria::confine_live_contract_tests::probe::<probe> --nocapture: the probe module (confine_live_contract_tests/probe.rs, declared mod probe; from the contract file) holds #[ignore] probe tests that read probe-input.json from their working directory and print one "PROBE <name> ok|err <detail>" line per check. Tests that set process variables are #[serial_test::serial] and restore them.
        WAVES: G1 and B1 in parallel, ONE message, both loom-senior-software-engineer with effort xhigh. Territories are DISJOINT. Workers NEVER spawn subagents. Each prompt: "Your brief: <path>. Read it in full before anything else."

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | G1 | Egress gate and live credential | opus/xhigh | src/lib.rs; src/live/** | common.md | doc/plans/briefs/live-test-lane/live-gate-and-boundary/g1-gate.md |
        | B1 | Live boundary, facilities, content digest | opus/xhigh | src/verify/criteria/confine_host.rs; src/verify/criteria/confine_bwrap.rs; src/verify/criteria/confine_seatbelt.rs; src/verify/criteria/live_boundary.rs; src/verify/criteria/runner_facilities.rs; src/verify/criteria/confine.rs; src/verify/criteria/executor.rs; src/verify/criteria/cache_fingerprint.rs; src/verify/criteria/zero_tests.rs; src/verify/criteria/mod.rs; src/verify/criteria/confine_host_tests.rs; src/verify/criteria/confine_host_tool_tests.rs; src/verify/criteria/confine_cargo_home_tests.rs; src/verify/criteria/confine_git_tests.rs | common.md | doc/plans/briefs/live-test-lane/live-gate-and-boundary/b1-boundary.md |

        AFTER THE WAVES: cargo fmt --all; cargo test --test maintainability; one loom check live-gate-and-boundary; the review rounds (one reviewer with the security-audit skill on the gate: secret check, header handling, SNI and Host, budget, upstream verification, log contents, credential file permissions; one on the boundary: the standard runner's argv unchanged, nothing beyond the gate reachable, the account home never writable, the facility list honest). No maintainability-baseline.txt row changes.
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "src/lib.rs"
        - "src/live/**"
        - "src/verify/criteria/**"
        - "tests/live_gate_contracts.rs"
        - "tests/live_gate_contracts/**"
      acceptance:
        - "cargo build"
        - "cargo test --lib live::"
        - "cargo test --lib verify::criteria::"
        - "cargo test --test live_gate_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
      before_stage:
        - command: "rg -q '^name = \"rcgen\"' Cargo.lock"
          description: "confinement-hardening added the gate's crates"
        - command: "rg -q 'pub fn require_live_profile' src/process/sandbox_probe/facilities.rs"
          description: "test-escape-policy defined the facility and profile guards"
      after_stage:
        - command: "rg -q 'pub fn run_live_with_timeout' src/verify/criteria/executor.rs"
          description: "the live boundary is exposed for the profile stage"
      artifacts:
        - "src/live/gate/mod.rs"
        - "src/live/credential.rs"
        - "src/verify/criteria/live_boundary.rs"
        - "src/verify/criteria/runner_facilities.rs"
      wiring:
        - source: "src/verify/criteria/confine_bwrap.rs"
          pattern: "live_bwrap_args("
          literal: true
          description: "the Linux runner adds the live arguments"
        - source: "src/verify/criteria/confine_seatbelt.rs"
          pattern: "live_seatbelt_rules("
          literal: true
          description: "the macOS runner adds the live rules"
        - source: "src/verify/criteria/executor.rs"
          pattern: "prepare_live("
          literal: true
          description: "a live run goes through the confined prepare path"
        - source: "src/verify/criteria/confine_host.rs"
          pattern: "runner_facilities("
          literal: true
          description: "every confined command carries the facility list"
      harness: ["tests/live_gate_contracts/**", "src/verify/criteria/confine_live_contract_tests/**"]
      contracts:
        - id: credential-load-refuses-symlink
          file: tests/live_gate_contracts.rs
          test: credential_load_refuses_symlink
          scenario: "a TempDir state root whose live-credential is a symlink to a 0600 file holding valid credential JSON"
          rejects: "a load that follows the link (any file the operator can read becomes the credential), instead of an Err"
        - id: credential-rejects-control-characters
          file: tests/live_gate_contracts.rs
          test: credential_rejects_control_characters
          scenario: "Credential::new with \"sk-test\\n\" (accepted, trimmed), \"sk test\", \"sk-te\\r\\nX-Evil: 1\" and \"\""
          rejects: "a value with a space, CR/LF or control byte reaching an HTTP header, instead of Ok(\"sk-test\") and Err for the other three"
        - id: credential-file-is-owner-only
          file: tests/live_gate_contracts.rs
          test: credential_file_is_owner_only
          scenario: "store into a fresh TempDir state root, stat the file; then chmod it 0644 and load"
          rejects: "a store that leaves the umask's mode or a load that accepts a group-readable file, instead of mode 0600 after store and Err after the chmod"
        - id: gate-refuses-connect-without-the-proxy-secret
          file: tests/live_gate_contracts.rs
          test: gate_refuses_connect_without_the_proxy_secret
          scenario: "a gate on Listen::Unix in a TempDir with Upstream::fixed to the fake upstream; one CONNECT api.anthropic.com:443 with no Proxy-Authorization and one with loom-live:wrong"
          rejects: "a gate that admits any client reaching its socket or port, instead of 407 for both, report.refused 2 and no request at the fake upstream"
        - id: gate-refuses-unlisted-connect-host
          file: tests/live_gate_contracts.rs
          test: gate_refuses_unlisted_connect_host
          scenario: "the right secret with CONNECT example.com:443, then CONNECT api.anthropic.com:8443"
          rejects: "a gate that tunnels any host or port it is asked for, instead of 403 for both and no upstream connection"
        - id: gate-loopback-listener-is-local-and-needs-the-secret
          file: tests/live_gate_contracts.rs
          test: gate_loopback_listener_is_local_and_needs_the_secret
          scenario: "a gate on Listen::Loopback; read endpoint(); CONNECT without the secret over TCP"
          rejects: "a loopback listener bound to 0.0.0.0, or one that trusts loopback clients without the secret, instead of Endpoint::Loopback with ip 127.0.0.1 and 407"
        - id: gate-injects-api-key
          file: tests/live_gate_contracts.rs
          test: gate_injects_api_key
          scenario: "an ApiKey credential REAL-KEY-1; after CONNECT with the secret the client sends POST /v1/messages with x-api-key: sk-ant-api03-loom-live-placeholder and authorization: Bearer guest"
          rejects: "a gate that forwards the client's headers or adds the key beside them, instead of the upstream seeing x-api-key REAL-KEY-1 exactly once and no authorization header"
        - id: gate-injects-oauth-bearer
          file: tests/live_gate_contracts.rs
          test: gate_injects_oauth_bearer
          scenario: "the same with an OauthToken credential REAL-OAT-1 and a request carrying authorization: Bearer sk-ant-oat01-loom-live-placeholder and anthropic-beta: oauth-2025-04-20"
          rejects: "an OAuth request sent with x-api-key or with the placeholder, instead of authorization: Bearer REAL-OAT-1, no x-api-key, and anthropic-beta passed through"
        - id: gate-refuses-host-header-mismatch
          file: tests/live_gate_contracts.rs
          test: gate_refuses_host_header_mismatch
          scenario: "CONNECT api.anthropic.com:443, TLS with SNI api.anthropic.com, then a request with Host: evil.example"
          rejects: "a gate that routes by Host after checking the tunnel (domain fronting with the real credential), instead of 421 and no upstream request"
        - id: gate-stops-at-request-budget
          file: tests/live_gate_contracts.rs
          test: gate_stops_at_request_budget
          scenario: "Budget { max_requests: 2, ..Default::default() } and three sequential requests"
          rejects: "an unbounded gate, instead of 200, 200, 429 and exactly two requests at the upstream"
        - id: gate-streams-the-response-before-it-ends
          file: tests/live_gate_contracts.rs
          test: gate_streams_the_response_before_it_ends
          scenario: "an upstream answering 200 with a chunked text/event-stream body: one event, then it waits for the test's signal before the second event and the end"
          rejects: "a gate that buffers the body until the upstream ends it (every streamed Claude Code turn stalls), instead of the client reading the first event while the upstream still holds the second"
        - id: gate-log-never-holds-secrets
          file: tests/live_gate_contracts.rs
          test: gate_log_never_holds_secrets
          scenario: "credential REAL-KEY-1; a request POST /v1/messages?key=QSECRET with body BODYSECRET; then stop"
          rejects: "a log of request URIs, headers, bodies or the proxy secret, instead of a line containing POST api.anthropic.com/v1/messages and no line containing QSECRET, BODYSECRET, REAL-KEY-1, the placeholder or proxy_secret()"
        - id: gate-refuses-untrusted-upstream
          file: tests/live_gate_contracts.rs
          test: gate_refuses_untrusted_upstream
          scenario: "the fake upstream's certificate signed by a CA that is not in the roots given to Upstream::fixed"
          rejects: "a gate that disables upstream verification and hands the credential to whoever answers, instead of 502 and no request reaching the fake upstream's handler"
        - id: live-bwrap-args-add-home-gate-and-extras-only
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::live_bwrap_args_add_home_gate_and_extras_only
          scenario: "live_bwrap_args for a LiveBoundary with home /h, GateMount::UnixDir(/g) and read_only [/bin/loom-x]"
          rejects: "a live builder that adds --share-net, binds /run (the host resolver and every host daemon socket) or binds the gate directory writable, instead of exactly --bind /h /h, --ro-bind /g /g, --ro-bind /bin/loom-x /bin/loom-x"
        - id: live-seatbelt-allows-only-the-gate-port
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::live_seatbelt_allows_only_the_gate_port
          scenario: "live_seatbelt_rules for home /h, tmp /t, GateMount::LoopbackPort(41234) and read_only holding one existing file (a NamedTempFile)"
          rejects: "an unscoped (allow network-outbound) or a localhost:* rule (every local service reachable), instead of exactly one (remote ip \"localhost:41234\") outbound rule, write rules for /h and /t and a literal read rule for the file"
        - id: live-profile-home-is-private-and-the-account-home-is-not-writable
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::live_profile_home_is_private_and_the_account_home_is_not_writable
          scenario: "the process HOME set to TempDir A (serial, restored); run_live_with_timeout of the probe in a TempDir tree with boundary.home = TempDir B/home; the probe writes $HOME/w, tries A/w, and prints HOME"
          rejects: "a boundary that binds the account home writable or leaves HOME at the account home, instead of the write under B/home succeeding, A/w refused and absent afterwards, and HOME equal to B/home"
        - id: live-profile-reaches-the-gate-and-nothing-else
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::live_profile_reaches_the_gate_and_nothing_else
          scenario: "the test serves one byte on a Unix socket in a TempDir gate directory (Linux, GateMount::UnixDir) or on a 127.0.0.1 port (macOS, LoopbackPort) and listens on a second 127.0.0.1 port; inside the boundary the probe connects to the gate, to the second port and to 192.0.2.1:443"
          rejects: "a profile that shares the host network or opens every loopback port, instead of the gate connect reading the byte and both other connects failing"
        - id: live-profile-environment-is-the-allowlist-plus-the-profile
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::live_profile_environment_is_the_allowlist_plus_the_profile
          scenario: "LOOM_CANARY_SECRET=x and HOME=TempDir A in the process (serial); boundary.env [(\"HTTPS_PROXY\", \"http://loom-live:s@127.0.0.1:3128\")], facilities [NetworkGate, WritableHome]; the probe prints its environment"
          rejects: "a profile that inherits the daemon's environment, or moves HOME without pinning RUSTUP_HOME and CARGO_HOME (toolchains then resolve under the private home), instead of no LOOM_CANARY_SECRET, HTTPS_PROXY present, RUSTUP_HOME equal to A/.rustup, CARGO_HOME set, and LOOM_TEST_FACILITIES naming network-gate and writable-home"
        - id: runner-advertises-only-what-it-delivers
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::runner_advertises_only_what_it_delivers
          scenario: "the standard confined runner (run_spec_in_optional_tree with CommandConfinement::Confined) runs the probe, which reads LOOM_TEST_FACILITIES and checks each advertised facility from inside: a Unix socket bind in its TMPDIR, a 127.0.0.1:0 bind, /proc/self/stat, and a bubblewrap start when bubblewrap is listed"
          rejects: "a fixed list the runner does not deliver, or bubblewrap advertised where nesting fails, instead of every listed facility's probe passing inside, unix-sockets, loopback and process-tree listed on Linux, and an empty list on macOS"
        - id: content-digest-ignores-commit-identity
          file: src/verify/criteria/confine_live_contract_tests.rs
          test: verify::criteria::confine_live_contract_tests::content_digest_ignores_commit_identity
          scenario: "a git fixture with one committed file and one uncommitted edit: content_digest, then commit the edit and digest again, then change one byte and digest"
          rejects: "a digest over HEAD or the index (a dirty-tree live run never matches the completion of the same bytes), instead of the first two digests equal and the third different"

    - id: live-profile
      name: "Live profile, commands and the live field"
      summary: "Runs one command end to end inside the live profile (seeded private home, gate, forwarder), adds the loom live commands (credential, status, smoke and the in-sandbox forwarder), and adds the live: stage field through validation, the stage record, the check-definition hash and the job deadline."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: ["live-gate-and-boundary"]
      description: |
        Build stage 4 of doc/plans/PLAN-live-test-lane.md (IN_PROGRESS- once running). Read its Decisions 3, 4, 7, 8 and 9 and doc/plans/briefs/live-test-lane/common.md first.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/; YAML paths are package-relative (../ reaches the repository root); brief paths are repo-relative.
        PUBLIC SURFACE (the contract session writes the contracts from this text before any code):
        - loom::live::profile: LIVE_PROXY_PORT: u16 = 3128; PLACEHOLDER_API_KEY = "sk-ant-api03-loom-live-placeholder"; PLACEHOLDER_OAUTH_TOKEN = "sk-ant-oat01-loom-live-placeholder"; ClaudeInstall { binary: PathBuf, version: String }; find_claude() -> Result<ClaudeInstall, String>; HomeSeed<'a> { run_id: &'a str, proxy_url: &'a str, ca_pem: &'a str, kind: CredentialKind, claude: Option<&'a ClaudeInstall> }; seed_home(home: &Path, seed: &HomeSeed) -> Result<()> writing .loom-live-profile (the run id), .loom-live/ca.pem, .claude.json ({"hasCompletedOnboarding": true} plus lastOnboardingVersion when claude is Some), .claude/settings.json (env with HTTPS_PROXY, https_proxy, HTTP_PROXY, http_proxy = proxy_url, NO_PROXY and no_proxy = "localhost,127.0.0.1,::1", NODE_EXTRA_CA_CERTS = <home>/.loom-live/ca.pem, CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC = "1", DISABLE_AUTOUPDATER = "1", and CLAUDE_CODE_OAUTH_TOKEN = PLACEHOLDER_OAUTH_TOKEN for OauthToken; apiKeyHelper = <home>/.loom-live/api-key-helper for ApiKey, that script printing PLACEHOLDER_API_KEY), and .local/bin/claude as a symlink to claude.binary when Some.
        - loom::live::entry: EntryRequest<'a> { loom_bin: &'a Path, tree: &'a Path, working_dir: &'a Path, command: &'a str, timeout: Duration, credential: &'a Credential, hosts: &'a [String], upstream: Upstream, claude: Option<&'a ClaudeInstall>, zero_test_guard: bool }; UnavailableCause { Credential, Facility } (serde kebab-case); EntryOutcome { Passed, Failed { reason: String }, Unavailable { cause: UnavailableCause, reason: String } } (serde tag "outcome", kebab-case); EntryReport { outcome: EntryOutcome, result: CriterionResult, gate: GateReport }; run_entry(&EntryRequest) -> Result<EntryReport> (Err only when the run root, gate or seeding fails; the command runs under run_live_with_timeout with HOME the run's private home, the gate as the only network path, LOOM_LIVE_PROFILE and the marker set, and on Linux through loom live exec --listen 127.0.0.1:3128; outputs scrubbed of the credential value). loom::live::verdict(command: &str, cwd: &Path, result: &CriterionResult, gate: &GateReport, credential_value: &str, zero_test_guard: bool) -> EntryOutcome in common.md's order (timed out, credential seen, budget, exit code, a loom-live-profile: skip line fails, a loom-live-profile: unavailable line is Unavailable { Facility }, zero tests when guarded, else Passed).
        - loom::live::forward::serve(listen: SocketAddr, gate: &Path) -> Result<Forwarder>; Forwarder::local_addr(&self) -> SocketAddr.
        - CLI (env!("CARGO_BIN_EXE_loom")): loom live exec --listen <addr> --gate <socket> -- <program> [args...] (hidden from --help; exits with the program's exit code); loom live credential set --kind <api-key|oauth-token> reads one line from stdin into <state root>/live-credential (mode 0600) and never prints the value; it and loom live credential clear exit non-zero and write nothing with LOOM_SESSION_ID, LOOM_STAGE_ID or LOOM_PLAN_SESSION set; loom live status prints lines starting boundary:, claude:, credential:, smoke: and exits 0; loom live --help lists credential, status and smoke.
        - loom::commands::live::smoke: SMOKE_COMMAND = "claude -p --max-turns 1 --model haiku 'Reply with the single word ready.'"; SmokeEnv { work_dir: PathBuf, loom_bin: PathBuf, upstream: Upstream, claude: Result<ClaudeInstall, String>, command: String, tree: PathBuf }; SmokeReceipt { passed: bool, finished_at: String, loom_version: String, platform: String, claude_version: Option<String>, reason: Option<String>, gate_requests: u32 } (serde); run_with(&SmokeEnv) -> Result<SmokeReceipt>: passed only when the entry passed and the gate logged a request to api.anthropic.com/v1/messages with status 200; writes <work_dir>/verification/live-smoke.json for every run, passed or not.
        - loom::plan::schema::{LiveEntry, LiveCredential, LIVE_PREPARE_SECS, LIVE_TIMEOUT_SECS, LIVE_MAX_ENTRIES, live_entry_hash}: LiveEntry { command: String, timeout_secs: u64 (default 1800), hosts: Vec<String> (default ["api.anthropic.com"]), credential: LiveCredential (default anthropic), required: bool (default false) } with deny_unknown_fields; LiveCredential { Anthropic } with hosts() -> &'static [&'static str]; LIVE_PREPARE_SECS = 60; LiveEntry::deadline_secs(&self) -> u64 = timeout_secs + 60; live_entry_hash(&LiveEntry) -> String (hex SHA-256; differs when any field differs).
        - StageDefinition.live: Vec<LiveEntry> (YAML key live, default empty); Stage.live: Vec<LiveEntry>, copied by loom::models::stage::Stage::from_definition(&StageDefinition, &PlanIdentity { id, version, ratchet_files }).
        - loom::plan::schema::validate(&LoomMetadata) -> Result<(), Vec<ValidationError>>: a v1 plan with live gives "`live` requires `version: 2`"; a v2 knowledge or knowledge-distill stage with live gives "`live` runs only on standard and integration-verify stages"; more than 8 entries gives "at most 8 `live` entries per stage"; a blank command gives "Live #n `command` cannot be empty"; timeout_secs outside 60..=7200 gives "Live #n timeout_secs <v> is outside 60..=7200"; empty hosts gives "Live #n `hosts` cannot be empty"; a host outside the credential's hosts gives "Live #n host `<h>` is not served for credential `anthropic`".
        - loom::handoff::check_definition_hash(&Stage) -> String changes when only stage.live changes; loom::host_verify::runner::job_deadline_secs(&Stage) -> u64 grows by each entry's deadline_secs.
        - loom plan verify --strict reports an escape error labelled "Live entry #n" for a live command using any escape but --include-ignored, and none for --include-ignored there.
        CONTRACT FILES: tests/live_profile_contracts.rs with harness tests/live_profile_contracts/** (reuse the fake upstream of tests/live_gate_contracts/ through a #[path] child module; probes run inside the profile by copying the test binary into the TempDir tree and running ./probe --ignored --exact <probe> --nocapture as the entry command; run_entry gets loom_bin = env!("CARGO_BIN_EXE_loom")), tests/live_cli_contracts.rs with harness tests/live_cli_contracts/** (a TempDir with .loom/work as the state root and cwd; the smoke contract passes SmokeEnv.command and tree pointing at the copied probe), tests/live_schema_contracts.rs with harness tests/live_schema_contracts/** (parse YAML with serde_saphyr::from_str::<loom::plan::schema::LoomMetadata>; the plan-verify contract writes a temp git repository holding a complete valid v2 PLAN.md, mirroring tests/escape_lint_contracts/support.rs).
        WAVES: P1, P2 and S1 in parallel, ONE message. P1 uses loom-senior-software-engineer (effort high); P2 and S1 use loom-software-engineer. Territories are DISJOINT. Workers NEVER spawn subagents. Each prompt: "Your brief: <path>. Read it in full before anything else."

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | P1 | Run root, HOME seeding, one entry run, forwarder | opus | src/live/mod.rs; src/live/profile.rs; src/live/run_root.rs; src/live/entry.rs; src/live/verdict.rs; src/live/forward.rs; src/live/tests_profile.rs | common.md | doc/plans/briefs/live-test-lane/live-profile/p1-profile.md |
        | P2 | loom live commands | sonnet | src/cli/types_live.rs; src/cli/types.rs; src/cli/types_tests.rs; src/cli/dispatch.rs; src/cli/mod.rs; src/commands/mod.rs; src/commands/live/mod.rs; src/commands/live/exec.rs; src/commands/live/credential.rs; src/commands/live/status.rs; src/commands/live/smoke.rs; src/commands/live/tests.rs; maintainability-baseline.txt | common.md | doc/plans/briefs/live-test-lane/live-profile/p2-cli.md |
        | S1 | live: field, validation, hash, deadline, escape scan | sonnet | src/plan/schema/types_live.rs; src/plan/schema/mod.rs; src/plan/schema/types.rs; src/plan/schema/validation/v2_fields.rs; src/plan/schema/validation/v2_fields/live.rs; src/plan/schema/validation/v2_lints/escape_hatches.rs; src/plan/schema/tests/v2_live_tests.rs; src/plan/schema/tests/mod.rs; src/models/stage/types.rs; src/models/stage/defaults.rs; src/models/stage/methods.rs; src/fs/stage_loading.rs; src/fs/tests_stage_loading_round_trip.rs; src/handoff/completion/identity.rs; src/host_verify/runner.rs | common.md | doc/plans/briefs/live-test-lane/live-profile/s1-schema.md |

        RATCHET: P2 lowers the exact row function src/cli/dispatch.rs dispatch 69 after folding the Project arm into a helper (a tightening). After the final review file ONE loom stage dispute-integrity live-profile for the TI-ratchet event (copy its id from loom stage review integrity live-profile) with the reason "dispatch shrank when the Project and Live arms moved into one helper; the row records the new exact length".
        AFTER THE WAVES: cargo fmt --all; cargo test --test maintainability (src/models/stage/types.rs stays 591 lines, src/models/stage/methods.rs 493, Stage::default 76: S1's brief says how); one loom check live-profile; the review rounds (one reviewer with the security-audit skill on the profile: nothing under the run root holds the credential, HOME seeding, the forwarder, the session refusal of credential set; one on the schema and CLI wiring).
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "src/live/**"
        - "src/cli/types_live.rs"
        - "src/cli/types.rs"
        - "src/cli/types_tests.rs"
        - "src/cli/dispatch.rs"
        - "src/cli/mod.rs"
        - "src/commands/mod.rs"
        - "src/commands/live/**"
        - "maintainability-baseline.txt"
        - "src/plan/schema/types_live.rs"
        - "src/plan/schema/mod.rs"
        - "src/plan/schema/types.rs"
        - "src/plan/schema/validation/v2_fields.rs"
        - "src/plan/schema/validation/v2_fields/live.rs"
        - "src/plan/schema/validation/v2_lints/escape_hatches.rs"
        - "src/plan/schema/tests/v2_live_tests.rs"
        - "src/plan/schema/tests/mod.rs"
        - "src/models/stage/types.rs"
        - "src/models/stage/defaults.rs"
        - "src/models/stage/methods.rs"
        - "src/fs/stage_loading.rs"
        - "src/fs/tests_stage_loading_round_trip.rs"
        - "src/handoff/completion/identity.rs"
        - "src/host_verify/runner.rs"
        - "tests/live_profile_contracts.rs"
        - "tests/live_profile_contracts/**"
        - "tests/live_cli_contracts.rs"
        - "tests/live_cli_contracts/**"
        - "tests/live_schema_contracts.rs"
        - "tests/live_schema_contracts/**"
      acceptance:
        - "cargo build"
        - "cargo test --lib live::"
        - "cargo test --lib commands::live::"
        - "cargo test --lib cli::types::tests::"
        - "cargo test --lib plan::schema::"
        - "cargo test --lib models::stage::"
        - "cargo test --lib fs::stage_loading::"
        - "cargo test --lib handoff::completion::"
        - "cargo test --lib host_verify::tests_runner"
        - "cargo test --test live_profile_contracts"
        - "cargo test --test live_cli_contracts"
        - "cargo test --test live_schema_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
      before_stage:
        - command: "rg -q 'pub fn start' src/live/gate/mod.rs"
          description: "live-gate-and-boundary merged the gate"
        - command: "rg -q 'pub fn run_live_with_timeout' src/verify/criteria/executor.rs"
          description: "live-gate-and-boundary merged the live boundary"
      after_stage:
        - command: "rg -q 'pub live: Vec<' src/models/stage/types.rs"
          description: "the stage record carries live entries"
      artifacts:
        - "src/live/entry.rs"
        - "src/live/profile.rs"
        - "src/live/forward.rs"
        - "src/plan/schema/types_live.rs"
        - "src/commands/live/smoke.rs"
      wiring:
        - source: "src/cli/dispatch.rs"
          pattern: "Commands::Live { .. }"
          literal: true
          description: "the CLI dispatches loom live"
        - source: "src/live/entry.rs"
          pattern: "run_live_with_timeout("
          literal: true
          description: "an entry runs inside the live boundary"
        - source: "src/commands/live/exec.rs"
          pattern: "forward::serve("
          literal: true
          description: "loom live exec starts the forwarder"
        - source: "src/handoff/completion/identity.rs"
          pattern: "live: &stage.live"
          literal: true
          description: "the check-definition hash binds the live entries"
        - source: "src/host_verify/runner.rs"
          pattern: "LiveEntry::deadline_secs"
          literal: true
          description: "the job deadline counts each live entry"
      harness: ["tests/live_profile_contracts/**", "tests/live_cli_contracts/**", "tests/live_schema_contracts/**"]
      contracts:
        - id: profile-request-reaches-the-upstream-through-the-gate
          file: tests/live_profile_contracts.rs
          test: profile_request_reaches_the_upstream_through_the_gate
          scenario: "run_entry with an ApiKey credential REAL-KEY-1, Upstream::fixed to the fake upstream, and a command running the probe that sends GET https://api.anthropic.com/v1/models through the HTTPS_PROXY in its environment, trusting the CA file NODE_EXTRA_CA_CERTS names"
          rejects: "a profile whose proxy or CA settings never reach the command, or a client path that bypasses the gate, instead of outcome Passed, exactly one request at the fake upstream carrying x-api-key REAL-KEY-1 and report.gate.requests 1"
        - id: profile-home-carries-placeholders-never-the-credential
          file: tests/live_profile_contracts.rs
          test: profile_home_carries_placeholders_never_the_credential
          scenario: "seed_home into two TempDir homes, one with CredentialKind::ApiKey and one with OauthToken, a fake ClaudeInstall and proxy_url http://loom-live:S@127.0.0.1:3128; read every file under each home"
          rejects: "seeding a credential value or the operator's own ~/.claude.json, or omitting the proxy or CA from settings.json env (a Claude Code started by loom's launch gets env -i), instead of the documented files, the matching placeholder, hasCompletedOnboarding true and HTTPS_PROXY and NODE_EXTRA_CA_CERTS in settings.json env"
        - id: profile-marker-reaches-the-command
          file: tests/live_profile_contracts.rs
          test: profile_marker_reaches_the_command
          scenario: "run_entry with a command running the probe that prints loom::process::sandbox_probe::in_live_profile(); the test process itself checks in_live_profile() too"
          rejects: "a profile that sets LOOM_LIVE_PROFILE but writes no marker, or writes it outside HOME, so every operator test skips inside the profile, instead of true inside and false in the test process"
        - id: verdict-refuses-a-run-whose-tests-skipped
          file: tests/live_profile_contracts.rs
          test: verdict_refuses_a_run_whose_tests_skipped
          scenario: "verdict for exit-0 results whose stderr holds loom-live-profile: skip t1: no marker; whose stderr holds loom-live-profile: unavailable t2: needs tmux, which this platform's live profile does not provide; whose stdout holds the credential value; whose stdout is cargo's running 0 tests summary for cargo test --test x with zero_test_guard; and a clean one"
          rejects: "trusting exit 0 alone (a profile run whose tests all skipped passes), instead of Failed, Unavailable { Facility }, Failed, Failed (selected zero tests) and Passed"
        - id: entry-timeout-kills-the-command-and-forwarder
          file: tests/live_profile_contracts.rs
          test: entry_timeout_kills_the_command_and_forwarder
          scenario: "run_entry with command sleep 600 and a 2 s timeout; afterwards look for any process whose command line names the run's gate socket or sleep 600"
          rejects: "a hard-coded timeout, or a forwarder left running after the deadline, instead of Failed with a timed-out reason within 20 seconds and no surviving process"
        - id: live-exec-exits-with-the-command
          file: tests/live_cli_contracts.rs
          test: live_exec_exits_with_the_command
          scenario: "the binary loom live exec --listen 127.0.0.1:0 --gate <TempDir socket path> -- sh -c 'exit 7'"
          rejects: "a forwarder that exits 0 or keeps running after its command ended, instead of exit code 7 within 10 seconds"
        - id: credential-set-reads-stdin-into-owner-only-file
          file: tests/live_cli_contracts.rs
          test: credential_set_reads_stdin_into_owner_only_file
          scenario: "runs the binary loom live credential set --kind api-key in a TempDir holding .loom/work, with LOOM_SESSION_ID, LOOM_STAGE_ID and LOOM_PLAN_SESSION removed and stdin sk-test-value plus a newline"
          rejects: "echoing the value or leaving the umask's mode, instead of .loom/work/live-credential with mode 0600, kind api-key, value sk-test-value, and neither stdout nor stderr containing sk-test-value"
        - id: credential-set-refused-inside-a-session
          file: tests/live_cli_contracts.rs
          test: credential_set_refused_inside_a_session
          scenario: "the same command with LOOM_SESSION_ID=s1 in its environment"
          rejects: "a stage session replacing the profile's credential, instead of a non-zero exit and no credential file"
        - id: smoke-receipt-records-the-outcome
          file: tests/live_cli_contracts.rs
          test: smoke_receipt_records_the_outcome
          scenario: "run_with with a stored credential, Upstream::fixed to the fake upstream, claude Ok(a fake install) and command running the copied probe that POSTs /v1/messages through the proxy; first with the upstream answering 200, then 401"
          rejects: "a receipt written only on success, or a smoke that passes on the command's exit code without a request having crossed the gate, instead of verification/live-smoke.json passed true after the first run and passed false with a reason after the second"
        - id: live-help-lists-subcommands
          file: tests/live_cli_contracts.rs
          test: live_help_lists_subcommands
          scenario: "runs the binary loom live --help"
          rejects: "commands built but never registered in the CLI, instead of exit 0 with credential, status and smoke in stdout and exec absent"
        - id: live-requires-version-2
          file: tests/live_schema_contracts.rs
          test: live_requires_version_2
          scenario: "a v1 LoomMetadata whose standard stage has live: [{command: \"true\"}], through validate"
          rejects: "a v1 plan silently carrying live entries, instead of the error `live` requires `version: 2` for that stage"
        - id: live-refused-on-knowledge-stages
          file: tests/live_schema_contracts.rs
          test: live_refused_on_knowledge_stages
          scenario: "a v2 plan whose knowledge-distill stage has a live entry, through validate"
          rejects: "validation that checks entry fields only, instead of the error that live runs only on standard and integration-verify stages"
        - id: live-entry-field-bounds
          file: tests/live_schema_contracts.rs
          test: live_entry_field_bounds
          scenario: "v2 standard stage entries with timeout_secs 59, 7201 and 1800, hosts [] and [\"example.com\"], and a blank command"
          rejects: "accepting a zero or unbounded timeout or an arbitrary egress host, instead of one error per bad value and none for timeout 1800"
        - id: check-hash-binds-live-entries
          file: tests/live_schema_contracts.rs
          test: check_hash_binds_live_entries
          scenario: "two Stage values from Stage::default() equal except stage.live[0].required (false vs true)"
          rejects: "a GatePolicy without live, so an edited entry still binds an old passing run, instead of different check_definition_hash values"
        - id: deadline-counts-live-timeouts
          file: tests/live_schema_contracts.rs
          test: deadline_counts_live_timeouts
          scenario: "job_deadline_secs for a Stage::default() and for the same stage with one live entry of timeout_secs 4000"
          rejects: "a deadline that ignores live entries, so the runner kills a live job mid-run, instead of a difference of exactly 4060"
        - id: live-entry-defaults-reach-the-stage
          file: tests/live_schema_contracts.rs
          test: live_entry_defaults_reach_the_stage
          scenario: "parses YAML live: [{command: \"true\"}, {command: \"x\", timeout_secs: 2400, required: true}] and builds Stage::from_definition"
          rejects: "a field that parses but is never copied into the stage record, or wrong defaults, instead of stage.live equal to the definition with the first entry at 1800, hosts [api.anthropic.com], credential anthropic and required false"
        - id: plan-verify-scans-live-commands-for-escapes
          file: tests/live_schema_contracts.rs
          test: plan_verify_scans_live_commands_for_escapes
          scenario: "a valid v2 plan whose standard stage has live entries cargo test --test x -- --include-ignored and claude --dangerously-skip-permissions -p x; runs the binary loom plan verify --strict"
          rejects: "live commands exempt from every escape check, or refused for --include-ignored, the flag live entries exist for, instead of one error naming --dangerously-skip-permissions and Live entry #2 and none naming Live entry #1"

    - id: live-checks
      name: "Live checks, receipts and completion"
      summary: "Adds loom check --live (a host job that runs a stage's live entries and records a receipt), the completion check that needs a current passing receipt for required entries, the block rule for a missing credential, the dispute route for live entries, a merge resolver's live run for the resolved tree, and the plan-writer doctrine."
      stage_type: standard
      skills: ["loom-rust", "loom-security-audit"]
      subagent_timeout_secs: 1200
      working_dir: "loom"
      dependencies: ["live-profile"]
      description: |
        Build stage 5 of doc/plans/PLAN-live-test-lane.md (IN_PROGRESS- once running). Read its Decisions 1-5 and doc/plans/briefs/live-test-lane/common.md ("Receipts and completion") first.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/; YAML paths are package-relative (../ reaches the repository root); brief paths are repo-relative.
        PUBLIC SURFACE (the contract session writes the contracts from this text before any code):
        - loom::host_verify::JobKind::Live (serde "live"); loom::relay::HostJobRequest gains live: bool (serde default false). Crate-internal orchestrator::core::inbox_drain::check::job_kind_for(request: &HostJobRequest, stage: &Stage) -> Result<JobKind, String>: Err naming --criterion when live and criteria is Some; Err naming the stage when live and stage.live is empty; Ok(Live) when live; Ok(Check) otherwise.
        - loom::live::receipt: ReceiptBinding { Content(String), CommitTree(String) }; ReceiptOutcome { Passed, Failed { reason: String }, Unavailable { cause: UnavailableCause, reason: String } } (serde tag "outcome"); ReceiptEntry { index: usize, command: String, required: bool, definition_hash: String, outcome: ReceiptOutcome, requests: u32, log: Option<String> }; LiveReceipt { stage_id: String, job_id: Option<String>, finished_at: String, loom_version: String, platform: String, binding: Option<ReceiptBinding>, entries: Vec<ReceiptEntry> }; current_binding(checkout: &Path) -> Result<Option<ReceiptBinding>> (Content when the content digest can be taken, else CommitTree of HEAD when the tree is clean, else None); save(work_dir: &Path, &LiveReceipt) -> Result<()> and load(work_dir: &Path, stage_id: &str) -> Result<Option<LiveReceipt>> at <work_dir>/verification/<stage>/live-receipt.json.
        - loom::live::stage_run: run_stage_entries_with(work_dir: &Path, stage: &Stage, checkout: &Path, job_id: Option<&str>, runner: &mut dyn FnMut(usize, &LiveEntry, &Credential) -> Result<EntryReport>) -> Result<LiveReceipt>: Err when current_binding is None; with no credential at the state root every entry is Unavailable { Credential } and runner is never called; otherwise runner per entry in order; a binding that differs after the run leaves receipt.binding None; the receipt is saved and returned. Crate-internal (StageExecutionPaths is pub(crate)): run_stage_entries(work_dir, stage, paths: &StageExecutionPaths, job_id, record: &mut dyn FnMut(usize, &LiveEntry, &EntryReport)) -> Result<LiveReceipt> runs the real entries.
        - loom::live::completion: LiveVerdict { failures: Vec<String>, advisories: Vec<String> }; evaluate(stage_id: &str, entries: &[LiveEntry], receipt: Option<&LiveReceipt>, current: Option<&ReceiptBinding>) -> LiveVerdict (a receipt counts only when its binding is Some and equals current; an entry's outcome is the receipt entry with its live_entry_hash; a required entry without one gives one failure containing "run loom check --live <stage>"; a required Failed gives a failure starting "[live <i>]"; a required Unavailable gives a failure starting "[live <i>]" that names its reason; an advisory entry never fails); block_refusal(stage_id: &str, failing_labels: &[String], entries: &[LiveEntry], receipt: Option<&LiveReceipt>, current: Option<&ReceiptBinding>) -> Option<String> (None when live is not failing or every failing required entry is Unavailable { Credential } in a counting receipt; else a refusal naming loom check --live or --field live); live_completion_check(stage: &Stage, checkout: &Path, work_dir: &Path) -> Result<()>; live_profile_note(stdout: &str, stderr: &str) -> Option<String> ("note: <n> tests need the live profile ..." counting distinct names from libtest's "test <name> ... ignored, loom-live-profile:" lines and the guard's "loom-live-profile: skip <name>:" lines).
        - Criterion output. Crate-internal commands::stage::criterion_output::format_criterion_result(index: usize, result: &CriterionResult, setup: &[String]) -> String (signature unchanged), calling live::completion::live_profile_note on result.stdout and result.stderr: a passing uncached result whose note is Some returns two lines, the pass line "  ✓ passed: <command>" then four spaces and the note; with None it returns exactly the pass line; a passing cached result returns exactly "  ✓ passed (cached): <command>" and never a note (the cache record keeps a 4 KiB tail of each stream, too short to count from; revisit when a record stores the count); a failing or timed-out result gets the same indented note as its last line, after the stderr and stdout tails.
        - CLI: loom check --live <stage> (conflicts with --criterion; --suggest refused); loom stage dispute-criteria <stage> --field live --criterion-index <n> --reason <r> parses (CriterionField::Live, AmendmentField::Live; a missing --criterion-index gives "--field live needs --criterion-index"); loom live status --stage <id>.
        - Amendment: loom::plan::amendment::apply_amendment with AmendmentField::Live and AmendmentPatch::Replace { index: 0, value: "command: \"x\"\nrequired: false" } rewrites the plan file's stage live[0] and the stage file's live[0]; a value that is not a LiveEntry fails before any write.
        - Merge resolution (Decision 16). Crate-internal orchestrator::signals::merge_live::format_live_section(stage: &Stage) -> String: empty unless stage.live holds an entry with required true; otherwise a section starting "## Live Entries" that holds "loom check --live <stage id>", says to run it after the merge commit and before "loom stage merge <stage id> --resolved" and again after any later commit, and names "loom stage block <stage id>" for an outcome the resolver cannot fix. orchestrator::signals::merge::format_merge_signal_content (pub(super), unchanged signature) appends it after the "## Your Task" section, so no signal of a stage without a required entry contains "loom check --live". loom::relay::verdict(SessionType::Merge, RequestKind::Check) is Apply. A relayed check from a Merge session record settles Applied with a JobKind::Live current job only when the payload has live true, the record is Running with merge_source_branch Some("loom/<stage id>"), and the stage is MergeConflict or MergeBlocked (orchestrator::core::inbox_drain::commit::require_resolver(work_dir, stage_id, record) -> Result<Stage, String>, the rule prepare_merge applies to a resolver's commit); otherwise it settles Refused and queues no job, with a reason containing "--live" when live is false.
        CONTRACT FILES: tests/live_checks_contracts.rs with harness tests/live_checks_contracts/** (git fixtures: git init -b main in a TempDir, local user, GIT_CONFIG_GLOBAL and GIT_CONFIG_SYSTEM pointed at absent files, GIT_CONFIG_NOSYSTEM=1; a TempDir state root; fake EntryReports built with CriterionResult::new and GateReport default values), src/orchestrator/core/inbox_drain/live_check_contract_tests.rs (loom derives the scaffold line in src/orchestrator/core/inbox_drain.rs; the drain-pass contract uses inbox_drain::test_support's fixture, entry_for and run_pass, and builds the s1 worktree the way check.rs's tests' repository helper does), src/orchestrator/signals/live_signal_contract_tests.rs (loom derives the scaffold line in src/orchestrator/signals/mod.rs; Session::new_merge for the session, Stage values with id s1, plan_version 2 and LiveEntry values spelled field by field), src/commands/stage/criterion_output_contract_tests.rs (loom derives the scaffold line in src/commands/stage/mod.rs; format_criterion_result is pub(crate), so this contract lives in the crate; CriterionResult::new for the results, with the cached field set after construction), tests/live_dispute_contracts.rs (runs env!("CARGO_BIN_EXE_loom")), src/plan/live_amendment_contract_tests.rs with harness src/plan/live_amendment_contract_tests/** (mirror the plan and stage-file fixtures of src/plan/tests/amendment.rs; loom derives the scaffold line in src/plan/mod.rs).
        FOUNDATION (you, before the wave): add pub mod completion; pub mod receipt; pub mod stage_run; to src/live/mod.rs and pub mod check; to src/commands/live/mod.rs, each new file holding one //! line naming its job.
        WAVES: R1, R2, R3 and R4 in parallel, ONE message. R1 uses loom-senior-software-engineer (effort high); R2, R3 and R4 use loom-software-engineer. Territories are DISJOINT. Workers NEVER spawn subagents. Each prompt: "Your brief: <path>. Read it in full before anything else."

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | R1 | loom check --live, the live job, receipts | opus | src/live/receipt.rs; src/live/stage_run.rs; src/live/tests_receipt.rs; src/commands/live/check.rs; src/host_verify/mod.rs; src/host_verify/child.rs; src/host_verify/child_live.rs; src/host_verify/submit.rs; src/host_verify/apply.rs; src/relay/payload.rs; src/orchestrator/core/inbox_drain/check.rs; src/orchestrator/core/inbox_drain/complete.rs; src/orchestrator/core/inbox_drain/commit.rs; src/relay/matrix_host.rs; src/relay/tests_matrix.rs; src/commands/verify.rs; src/commands/verify_relay.rs; src/cli/dispatch.rs | common.md | doc/plans/briefs/live-test-lane/live-checks/r1-live-runs.md |
        | R2 | Completion check, block rule, profile-test note, status, resolver signal | sonnet | src/live/completion.rs; src/live/tests_completion.rs; src/commands/stage/complete_verification.rs; src/stage_service/control_block.rs; src/stage_service/control_block_agent_tests.rs; src/commands/stage/criterion_output.rs; src/cli/types_live.rs; src/commands/live/mod.rs; src/commands/live/status.rs; src/orchestrator/signals/merge.rs; src/orchestrator/signals/merge_live.rs; src/orchestrator/signals/merge_live_tests.rs; src/orchestrator/signals/mod.rs | common.md | doc/plans/briefs/live-test-lane/live-checks/r2-completion-and-block.md |
        | R3 | The --field live dispute route | sonnet | src/models/dispute.rs; src/plan/amendment.rs; src/plan/amendment_fields.rs; src/plan/amendment/journal.rs; src/plan/amendment_catch_up.rs; src/cli/types_stage_disputes.rs; src/cli/types_stage_amend.rs; src/stage_service/dispute.rs; src/models/stage/dispute_budgets.rs; src/orchestrator/adjudication/verdict.rs; src/orchestrator/adjudication/prompt.rs; src/orchestrator/adjudication/prompt/criterion_entry.rs; src/orchestrator/adjudication/apply.rs; src/orchestrator/adjudication/apply_accept.rs; src/orchestrator/adjudication/plan_patch.rs; src/daemon/protocol_debug.rs; src/verify/goal_backward/mod.rs | common.md | doc/plans/briefs/live-test-lane/live-checks/r3-live-dispute.md |
        | R4 | Plan-writer doctrine | sonnet | ../skills/loom-plan-writer/SKILL.md; ../skills/loom-plan-writer/references/sandbox.md; ../skills/loom-plan-writer/references/verification-rules.md; ../skills/loom-plan-writer/references/v2-contracts.md | common.md | doc/plans/briefs/live-test-lane/live-checks/r4-doctrine.md |

        A compile error in a file no row owns (an exhaustive match on JobKind or CriterionField) goes to the row whose change caused it; record the file in loom memory.
        AFTER THE WAVES: cargo fmt --all; one loom check live-checks; the review rounds (one reviewer with the security-audit skill on authority: only the host child and the operator write receipts, completion never runs a live entry, the block rule cannot park a fixable failure, a Merge session's check is admitted only as the live run of the stage it resolves; one on the dispute route, the resolver signal and the doctrine). No maintainability-baseline.txt row changes. If integrity events arise (control_block agent tests whose refusal text changed), dispute them in ONE filing citing Decision 5.
        MEMORY: record mistakes, decisions and surprises via loom memory immediately (subagents too); NEVER loom knowledge; NEVER Claude Code auto-memory.
      files:
        - "src/live/**"
        - "src/commands/live/**"
        - "src/host_verify/mod.rs"
        - "src/host_verify/child.rs"
        - "src/host_verify/child_live.rs"
        - "src/host_verify/submit.rs"
        - "src/host_verify/apply.rs"
        - "src/relay/payload.rs"
        - "src/orchestrator/core/inbox_drain.rs"
        - "src/orchestrator/core/inbox_drain/check.rs"
        - "src/orchestrator/core/inbox_drain/complete.rs"
        - "src/orchestrator/core/inbox_drain/commit.rs"
        - "src/orchestrator/core/inbox_drain/live_check_contract_tests.rs"
        - "src/relay/matrix_host.rs"
        - "src/relay/tests_matrix.rs"
        - "src/orchestrator/signals/mod.rs"
        - "src/orchestrator/signals/merge.rs"
        - "src/orchestrator/signals/merge_live.rs"
        - "src/orchestrator/signals/merge_live_tests.rs"
        - "src/orchestrator/signals/live_signal_contract_tests.rs"
        - "src/commands/verify.rs"
        - "src/commands/verify_relay.rs"
        - "src/cli/dispatch.rs"
        - "src/cli/types_live.rs"
        - "src/commands/stage/complete_verification.rs"
        - "src/commands/stage/mod.rs"
        - "src/commands/stage/criterion_output.rs"
        - "src/commands/stage/criterion_output_contract_tests.rs"
        - "src/stage_service/control_block.rs"
        - "src/stage_service/control_block_agent_tests.rs"
        - "src/models/dispute.rs"
        - "src/plan/mod.rs"
        - "src/plan/amendment.rs"
        - "src/plan/amendment_fields.rs"
        - "src/plan/amendment/journal.rs"
        - "src/plan/amendment_catch_up.rs"
        - "src/plan/live_amendment_contract_tests.rs"
        - "src/plan/live_amendment_contract_tests/**"
        - "src/cli/types_stage_disputes.rs"
        - "src/cli/types_stage_amend.rs"
        - "src/stage_service/dispute.rs"
        - "src/models/stage/dispute_budgets.rs"
        - "src/orchestrator/adjudication/**"
        - "src/daemon/protocol_debug.rs"
        - "src/verify/goal_backward/mod.rs"
        - "tests/live_checks_contracts.rs"
        - "tests/live_checks_contracts/**"
        - "tests/live_dispute_contracts.rs"
        - "../skills/loom-plan-writer/SKILL.md"
        - "../skills/loom-plan-writer/references/sandbox.md"
        - "../skills/loom-plan-writer/references/verification-rules.md"
        - "../skills/loom-plan-writer/references/v2-contracts.md"
      acceptance:
        - "cargo build"
        - "cargo test --lib live::"
        - "cargo test --lib host_verify::"
        - "cargo test --lib orchestrator::core::inbox_drain::"
        - "cargo test --lib orchestrator::signals::"
        - "cargo test --lib relay::matrix::"
        - "cargo test --lib stage_service::"
        - "cargo test --lib commands::stage::complete::complete_verification"
        - "cargo test --lib commands::stage::criterion_output"
        - "cargo test --lib plan::"
        - "cargo test --lib orchestrator::adjudication::"
        - "cargo test --test live_checks_contracts"
        - "cargo test --test live_dispute_contracts"
        - "cargo test --test maintainability"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps"
        - 'rg -qF -- "loom check --live" ../skills/loom-plan-writer/SKILL.md'
      before_stage:
        - command: "rg -q 'pub fn run_entry' src/live/entry.rs"
          description: "live-profile merged run_entry"
        - command: "rg -q 'pub live: Vec<' src/models/stage/types.rs"
          description: "live-profile merged the live field"
      after_stage:
        - command: "rg -q 'Live =>' src/host_verify/child.rs"
          description: "the host child runs live jobs"
      artifacts:
        - "src/live/receipt.rs"
        - "src/live/stage_run.rs"
        - "src/live/completion.rs"
        - "src/host_verify/child_live.rs"
        - "src/orchestrator/signals/merge_live.rs"
      wiring:
        - source: "src/host_verify/child.rs"
          pattern: "child_live::run_live_job("
          literal: true
          description: "the host child runs a live job"
        - source: "src/commands/stage/complete_verification.rs"
          pattern: "live_completion_check("
          literal: true
          description: "completion checks the live receipt"
        - source: "src/stage_service/control_block.rs"
          pattern: "block_refusal("
          literal: true
          description: "the block guard applies the live rule"
        - source: "src/commands/stage/criterion_output.rs"
          pattern: "live_profile_note("
          literal: true
          description: "criterion output calls live::completion::live_profile_note to count the tests that need the live profile, for a passing criterion as well as a failing one"
        - source: "src/orchestrator/core/inbox_drain/check.rs"
          pattern: "job_kind_for("
          literal: true
          description: "a relayed check with live becomes a live job"
        - source: "src/cli/dispatch.rs"
          pattern: "args.live"
          literal: true
          description: "loom check --live is routed"
        - source: "src/orchestrator/signals/merge.rs"
          pattern: "merge_live::format_live_section("
          literal: true
          description: "the merge signal carries the resolver's live section"
        - source: "src/orchestrator/core/inbox_drain/check.rs"
          pattern: "require_resolver("
          literal: true
          description: "a Merge session's check is admitted only from the stage's resolver"
      reachable:
        - symbol: run_stage_entries
          from: run_job
          description: "the host child reaches the live run"
        - symbol: live_completion_check
          from: run_checks
          description: "the completion checks reach the live receipt check"
      harness: ["tests/live_checks_contracts/**", "src/plan/live_amendment_contract_tests/**"]
      contracts:
        - id: live-check-relays-as-a-live-job
          file: src/orchestrator/core/inbox_drain/live_check_contract_tests.rs
          test: orchestrator::core::inbox_drain::live_check_contract_tests::live_check_relays_as_a_live_job
          scenario: "job_kind_for with live true and criteria None on a stage with one live entry; live true with criteria Some([0]); live true on a stage with no live entries; live false"
          rejects: "treating --live as an ordinary check (it would run acceptance) or queueing a live job for a stage with nothing to run, instead of Ok(Live), Err, Err and Ok(Check)"
        - id: live-receipt-binds-content-and-entry
          file: tests/live_checks_contracts.rs
          test: live_receipt_binds_content_and_entry
          scenario: "a git fixture worktree with an uncommitted file, a stored credential, a stage with two live entries; run_stage_entries_with a runner returning Passed for entry 0 and Failed for entry 1; then load the receipt"
          rejects: "a receipt bound to HEAD or to nothing, or without each entry's definition hash, instead of binding Content equal to current_binding(checkout) and entries carrying live_entry_hash of each entry with Passed and Failed"
        - id: missing-credential-is-unavailable-and-runs-nothing
          file: tests/live_checks_contracts.rs
          test: missing_credential_is_unavailable_and_runs_nothing
          scenario: "the same fixture with no live-credential at the state root; a runner that records whether it was called"
          rejects: "skipping (a pass) or running the entries without the gate, instead of every entry Unavailable { Credential } and the runner never called"
        - id: tree-change-during-a-live-run-voids-the-binding
          file: tests/live_checks_contracts.rs
          test: tree_change_during_a_live_run_voids_the_binding
          scenario: "a runner that writes a new file into the worktree before returning Passed"
          rejects: "binding the digest taken before the run, so completion accepts content no entry saw, instead of receipt.binding None"
        - id: completion-needs-a-current-passing-receipt-for-required-entries
          file: tests/live_checks_contracts.rs
          test: completion_needs_a_current_passing_receipt_for_required_entries
          scenario: "evaluate for a required and an advisory entry with: no receipt; a receipt for other content; a current receipt with the required entry Passed and the advisory one Failed; a current receipt with the required entry Unavailable { Credential }; a current receipt whose required entry carries another definition hash"
          rejects: "completion running live entries itself, accepting any passing receipt whatever content it saw, or failing on an advisory entry, instead of a run loom check --live failure, the same failure, no failure with one advisory line, a [live 0] failure naming the credential, and the run loom check --live failure"
        - id: block-allowed-only-for-a-missing-credential
          file: tests/live_checks_contracts.rs
          test: block_allowed_only_for_a_missing_credential
          scenario: "block_refusal with failing labels [live] for a current receipt whose required entry is Unavailable { Credential }; is Unavailable { Facility }; is Failed; for no receipt; and with failing labels [after-stage] only"
          rejects: "a guard that lets every live failure block (parking work a rerun or a dispute settles) or refuses the credential case (a person's need with no other route), instead of None, a refusal naming --field live, a refusal naming --field live, a refusal naming loom check --live, and None"
        - id: criterion-output-counts-profile-tests
          file: tests/live_checks_contracts.rs
          test: criterion_output_counts_profile_tests
          scenario: "live_profile_note with stdout holding test a ... ignored, loom-live-profile: drives Claude Code, test b ... ignored, loom-live-profile: drives Claude Code and test c ... ignored, slow, and stderr holding loom-live-profile: skip d: no marker and loom-live-profile: skip a: no marker"
          rejects: "counting every ignored test, or counting a test twice, or none, instead of a note naming 3 tests"
        - id: passing-criterion-output-carries-the-profile-note
          file: src/commands/stage/criterion_output_contract_tests.rs
          test: commands::stage::criterion_output_contract_tests::passing_criterion_output_carries_the_profile_note
          scenario: "format_criterion_result(0, result, &[]) for four CriterionResult::new results of command cargo test with an empty stderr, a duration of 1 ms, success true and exit code Some(0) unless stated: (1) stdout holding test a ... ignored, loom-live-profile: drives Claude Code; (2) the same result with its cached field set to true; (3) stdout holding test b ... ok only; (4) success false with exit code Some(1), stderr boom and the stdout of (1)"
          rejects: "a note computed but never reaching a passing criterion's output (it exists for passing runs, and only a failing run printed it), or shown on a cached pass counted from a 4 KiB tail, or a pass line changed when no test needs the profile, or a failing run's note placed before its tails, instead of (1) exactly two lines, the first '  ✓ passed: cargo test' and the second four spaces followed by the text live_profile_note returns for that stdout, which starts 'note: 1 tests need the live profile', (2) exactly '  ✓ passed (cached): cargo test', (3) exactly '  ✓ passed: cargo test', and (4) output whose last line is that same indented note and which holds 'stderr:' and 'stdout:' before it"
        - id: live-dispute-field-is-accepted-by-the-cli
          file: tests/live_dispute_contracts.rs
          test: live_dispute_field_is_accepted_by_the_cli
          scenario: "the binary loom stage dispute-criteria s1 --field live --reason x in a TempDir"
          rejects: "a dispute route without a live field (clap rejects 'live' as an invalid value), instead of a non-zero exit whose stderr says --field live needs --criterion-index and does not say invalid value"
        - id: accepted-live-amendment-rewrites-the-entry
          file: src/plan/live_amendment_contract_tests.rs
          test: plan::live_amendment_contract_tests::accepted_live_amendment_rewrites_the_entry
          scenario: "a work dir with a v2 plan whose stage has live [{command: \"cargo test\", required: true}] and its stage file; apply_amendment with AmendmentField::Live Replace index 0 value command x required false; then one with a value missing command"
          rejects: "a live field the amendment path cannot reach, or one that writes before validating, instead of plan and stage file both showing live[0] command x required false, and the second call Err with both files unchanged"
        - id: resolution-voids-the-pre-merge-receipt
          file: tests/live_checks_contracts.rs
          test: resolution_voids_the_pre_merge_receipt
          scenario: "a git fixture whose main holds a.txt and b.txt and whose checkout is on loom/s1 with one commit changing b.txt; a stored credential; a v2 stage s1 with one required live entry; run_stage_entries_with a Passed runner on the checkout; commit a change to a.txt on main and run git merge --no-ff --no-edit main in the checkout (the resolved merge); live_completion_check(stage, checkout, state root); then run_stage_entries_with a Passed runner again and live_completion_check again"
          rejects: "a binding over the branch's pre-merge commit or the merge's first parent, or a completion that accepts any passing receipt of the stage, so a resolution lands content no live run saw, instead of Err containing run loom check --live s1 after the merge and Ok after the run on the merged tree"
        - id: resolver-may-request-only-its-own-live-run
          file: src/orchestrator/core/inbox_drain/live_check_contract_tests.rs
          test: orchestrator::core::inbox_drain::live_check_contract_tests::resolver_may_request_only_its_own_live_run
          scenario: "four fresh inbox_drain fixtures, each with a git repository holding the .worktrees/s1 worktree on loom/s1 and stage s1 saved MergeConflict with plan_version 2 and one live entry; in each one run_pass settles one relayed check carrying the worktree's head: (1) live true from a Merge record, Running, merge_source_branch loom/s1; (2) live false from that record; (3) live true from a Merge record whose merge_source_branch is loom/other; (4) live true from the first record after s1 is saved Executing"
          rejects: "a relay that admits any Merge session's check (a resolver then runs acceptance, or another stage's live entries) or still refuses the resolver's own live run, instead of Applied with a current JobKind::Live job in (1), and Refused with no current job in (2), (3) and (4), the reason in (2) containing --live"
        - id: resolver-signal-asks-for-a-live-run-only-with-required-entries
          file: src/orchestrator/signals/live_signal_contract_tests.rs
          test: orchestrator::signals::live_signal_contract_tests::resolver_signal_asks_for_a_live_run_only_with_required_entries
          scenario: "format_merge_signal_content with Session::new_merge(loom/s1, main), source loom/s1, target main, conflicting files [src/a.rs] and no protected paths, for a v2 stage s1 whose live holds one entry with required true; then the same entry with required false; then live empty"
          rejects: "a signal that always or never asks for the live run, or keys on any live entry instead of a required one, so a resolver either lands into a guaranteed live refusal or spends the credential for nothing, instead of the first signal holding ## Live Entries and loom check --live s1 after ## Your Task, and neither other signal containing loom check --live"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Confirms the live profile, the live checks and the hardening are wired into the running program and the full suite passes, then asks for the one real API call through the profile that no stage can make."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-security-audit"]
      working_dir: "."
      dependencies: ["live-checks"]
      description: |
        Final verification of doc/plans/IN_PROGRESS-PLAN-live-test-lane.md. Verify FUNCTIONAL INTEGRATION, not only tests passing. NEVER Claude Code auto-memory.
        Use parallel subagents and skills to maximize performance.
        CONTEXT: read the plan (The runner read from the code, Decisions, Environment inventory, Unverified external facts, Residual risks), doc/plans/briefs/live-test-lane/common.md, loom memory show --all, and the knowledge sections the plan cites.
        BUILD AND TEST (zero tolerance; nothing is pre-existing): every acceptance command below, through one loom check integration-verify.
        CODE REVIEW: parallel loom-code-reviewer subagents: (1) security with the security-audit skill: the gate (secret, headers, Host, budget, upstream verification, log contents), the credential file and every capsule's deny of it, the boundary (only the gate reachable, the account home never writable, the facility list honest), HOME seeding never holding the credential, receipts written only by the host child or the operator, the escape lint's coverage, the codex trust-surface denies on Linux and macOS; (2) architecture: JobKind::Live in submit, apply and the deadline, completion never running a live entry, the block rule, the dispute route, a merge resolver's live run (its signal section and the relay admitting only its own stage's live check), the confine_host.rs split leaving the standard runner unchanged, the removals and the test-only variant; (3) test coverage: each contract against the wrong implementation it names. Fix or dispute every finding; never defer one.
        FUNCTIONAL (one Bash invocation): build the binary (cargo build --manifest-path loom/Cargo.toml); bin=$PWD/loom/target/debug/loom; "$bin" live status prints the four lines and names what is missing; "$bin" live --help lists credential, status and smoke; in H=$(mktemp -d "${TMPDIR:-/tmp}/live.XXXXXX") write a v2 plan whose standard stage has live: [{command: "cargo test", timeout_secs: 30}], one whose stage sandbox sets command_confinement: inherit, one with acceptance "cargo test -- --include-ignored", and one with live: [{command: "claude --dangerously-skip-permissions -p x"}], and run "$bin" plan verify --strict on each: the timeout bound, the inherit parse error, the escape and the live escape are each reported. Record the transcript summary in loom memory.
        REAL CALL (after the confined gates and the review rounds are clean, before any loom stage complete): run loom live status and record it; then loom stage block integration-verify "The live profile needs one real API call through the gate, which no confined stage can make. From the repository root run: bash ./dev-install.sh; then loom live credential set --kind api-key or --kind oauth-token with a dedicated revocable credential on stdin (claude setup-token output, or a spend-capped Console API key); then loom live smoke. The smoke names whatever is missing (bubblewrap or sandbox-exec, a native Claude Code, the credential). Retry this stage after a passing smoke." On the retried session run loom live status: complete only when it shows smoke: passed with the installed loom version (loom --version) and a time after the block; a failed smoke is a finding to fix here (an engineer agent, then loom check, then block again with the same text).
        Record discoveries to loom memory for knowledge-distill, including any knowledge file contradicted by the tree: loom memory note "stale-knowledge: ...".
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - command: "cargo test --manifest-path loom/Cargo.toml --doc --workspace --all-features -- --test-threads=1"
          timeout_secs: 1200
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --check"
        - "RUSTDOCFLAGS='-D warnings' cargo doc --manifest-path loom/Cargo.toml --workspace --all-features --no-deps"
      wiring:
        - source: "loom/src/commands/stage/complete_verification.rs"
          pattern: "live_completion_check("
          literal: true
          description: "completion checks the live receipt"
        - source: "loom/src/host_verify/child.rs"
          pattern: "child_live::run_live_job("
          literal: true
          description: "the host child runs live jobs"
        - source: "loom/src/cli/dispatch.rs"
          pattern: "Commands::Live { .. }"
          literal: true
          description: "loom live is dispatched"
        - source: "loom/src/verify/criteria/confine.rs"
          pattern: "apply_confined_host_environment("
          literal: true
          description: "confined criteria take the confined allowlist"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records what this plan learned about the live profile and the confinement hardening in the knowledge base and documents the live profile in the README."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge; update user docs. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries; keep code spot-reads narrow.
        START with loom memory pending --group (corrections, mistakes, decisions, other, suggestions); read the plan and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge memory in place with loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update. Expected among them: concerns/sandbox-and-confinement-gaps.md "Confined Commands Still Reach a Live Credential Bus" is resolved (loom knowledge delete-section, and rewrite architecture/execution-containment.md "The Host Environment Allowlist" to the two lists); "The Codex Lane Grants All of ~/.codex" (rewrite to the current denies and the open decision on skills and rules); "Criteria Run Outside the Session Sandbox" (the runner permits nested bubblewrap, AF_UNIX under its private /tmp and loopback, and advertises them in LOOM_TEST_FACILITIES); "Three Stage Environment Allowlists"; execution-containment.md "The Two Confinement Levels" (one level for plans, the test-only variant) and "Confinement E2E Lives Outside the Sandbox"; planning-session-sandbox.md "Confinement test"; every section telling a reader to set LOOM_TEST_REQUIRE_SANDBOX_FREE or LOOM_E2E_REQUIRE_TMUX (loom knowledge context --query "LOOM_TEST_REQUIRE_SANDBOX_FREE" lists them; history stays only in mistakes files).
        Then curate: a new topic architecture/live-profile (loom knowledge update architecture/live-profile, then loom knowledge annotate architecture/live-profile --blurb "Live profile: gated egress, receipts, test guards") covering the boundary on both platforms, the gate, the credential file, HOME seeding, loom check --live and JobKind::Live, receipts and the completion rule, the block and dispute routes, the facility and profile guards, the unverified external facts and which the smoke settled; a 2-4 line summary plus link in architecture.md; the escape lint and the removals in the verification or sandbox topics they belong to; mistakes with prevention rules. TIER ROUTING: findings ~40 lines or fewer go inline in the tier-1 file; larger ones via loom knowledge update <category>/<slug>.
        INDEX SIZE: add at most the one new topic row, keep blurbs short (loom knowledge annotate --blurb), fold findings into existing topics where they fit; never delete a topic and never use --write-baseline to absorb a size issue.
        DOCS: add a "Live profile" section to README.md (live: entries and their fields, loom check --live, receipts and the completion rule, loom live credential set, status and smoke, the prerequisites on Linux and macOS, what macOS's profile does not provide). Lint with web/node_modules/.bin/markdownlint-cli2 README.md WITHOUT --fix; a non-zero exit with no "markdownlint-cli2 v" banner means the tool never ran.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then resolve it promoted, merged or discarded.
        RECEIPTS: every memory taken into knowledge gets loom memory resolve <id> --outcome promoted|merged|discarded|deferred right after the write that used it; finish with loom memory pending --strict.
        LAST, if this stage removed structural issues: loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
        - "web/node_modules/.bin/markdownlint-cli2 README.md"
      files: ["doc/loom/knowledge/**", "README.md"]
```

<!-- END loom METADATA -->
