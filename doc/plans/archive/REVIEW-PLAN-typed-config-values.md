# Code Review: Typed Configuration Values

**Plan:** PLAN-typed-config-values | **Generated:** 2026-09-16 15:16 UTC

## Summary

## Overview

## Changes by Stage

### Typed Configuration Values (typed-config-values)

**Status:** completed  
**Purpose:** Give the user-config registry a typed value that survives to every
surface, and give the TUI per-kind editing. Today KeySpec::parse
(loom/src/user_config/keys.rs:186-218) produces a real typed
toml_edit::Value on the WRITE path, but UserConfig::value_of
(loom/src/user_config/render.rs:17-43) returns (String, Origin) on the
READ path, so the CLI, the TUI, the JSON wire and the TypeScript client
all handle strings and the TS side re-derives the type it was never
sent.

Use parallel subagents and skills to maximize performance.

FIVE WORKERS IN THREE WAVES. Territories are DISJOINT. Workers NEVER
spawn subagents. Spawn each wave's workers BY AGENT TYPE, ALL in ONE
message, each with the fixed prompt plus the line
"Your brief: <path>. Read it in full before anything else."

Wave 1: W1. Wave 2: W2 and W3, after W1 returns. Wave 3: W4 and W5,
after W2 and W3 return.

| Worker | Role | Tier | Files owned | Brief path |
| --- | --- | --- | --- | --- |
| W1 | typed value core, user_config, CLI | sonnet (loom-software-engineer) | loom/src/user_config/**, loom/src/user_config/value.rs, loom/src/user_config/tests/value.rs, loom/src/commands/config/mod.rs, loom/src/commands/config/tests.rs | doc/plans/briefs/typed-config-values/typed-config-values/w1-core.md |
| W2 | TUI per-kind editing | codex gpt-5.6-terra, units w2a-state, w2b-keys-render, w2c-tests | loom/src/commands/config/tui/**, loom/src/commands/config/tui/tests/cycling.rs | doc/plans/briefs/typed-config-values/typed-config-values/w2-tui.md |
| W3 | dashboard Rust API + JSON fixture | codex gpt-5.6-terra, units w3a-wire-entries, w3b-handlers-workspace, w3c-existing-tests, w3d-typed-values-fixture | loom/src/commands/status/web/config_api.rs, loom/src/commands/status/web/config_api/wire.rs, loom/src/commands/status/web/config_api/entries.rs, loom/src/commands/status/web/config_api/apply.rs, loom/src/commands/status/web/config_api/workspace.rs, loom/src/commands/status/web/config_api/tests.rs, loom/src/commands/status/web/config_api/tests/resolution.rs, loom/src/commands/status/web/config_api/tests/updates.rs, loom/src/commands/status/web/config_api/tests/typed_values.rs, loom/src/commands/status/web/tests/config_api.rs, web/src/api/fixtures/config.json | doc/plans/briefs/typed-config-values/typed-config-values/w3-web-api.md |
| W4 | TS wire schema + settings model | codex gpt-5.6-terra, one unit w4-web-model | web/src/api/config.ts, web/src/components/settings-model.ts, web/src/components/settings-model.test.ts | doc/plans/briefs/typed-config-values/typed-config-values/w4-web-model.md |
| W5 | React controls + test kit | codex gpt-5.6-terra, units w5a-control-kit, w5b-cards-lanes, w5c-dialog-tests, w5d-number-field | web/src/components/settings-control.tsx, web/src/components/settings-cards.tsx, web/src/components/settings-dialog.tsx, web/src/components/settings-lanes.tsx, web/src/components/settings-lanes-cells.tsx, web/src/test/settings-kit.tsx, web/src/components/settings-cards.test.tsx, web/src/components/settings-dialog.test.tsx, web/src/components/settings-entry.test.tsx, web/src/components/settings-string-field.test.tsx, web/src/components/settings-number-field.test.tsx | doc/plans/briefs/typed-config-values/typed-config-values/w5-web-controls.md |

CODEX LANE. Spawn W2-W5 as loom-codex-forwarder subagents in the
FOREGROUND, each with --model gpt-5.6-terra --effort xhigh and an
explicit Bash timeout of 600000 ms. A codex worker cannot see edits made
during this run: its source-graph lookups answer from the published base
layer. W2 and W3 therefore get the full text of W1's new
loom/src/user_config/value.rs contract inside their briefs rather than
being told to look it up, and W5 gets W4's TypeScript contract the same
way.

ONE FORWARD PER UNIT, NOT PER WORKER. The wrapper cancels a unit at
540000 ms, so a territory is forwarded as a sequence of units of at most
three files and three steps each (W5's brief marks its two four-file
units, whose extra files are one-line type retypes, and says where to
re-split them). Assign the unit id in the prompt with
--unit-id and never invent one: W4 is the single unit w4-web-model; W2
is w2a-state, w2b-keys-render, w2c-tests; W3 is w3a-wire-entries,
w3b-handlers-workspace, w3c-existing-tests, w3d-typed-values-fixture;
W5 is w5a-control-kit, w5b-cards-lanes, w5c-dialog-tests,
w5d-number-field. Each brief's
"Codex units" section names the files a unit writes, its steps, and the
interface the next unit compiles against. The units of ONE worker run
SEQUENTIALLY, one loom-codex-forwarder spawn each; W2 and W3 still run
concurrently with each other, and so do W4 and W5. An exit 124 means
the unit exceeded the deadline and was cancelled: RE-SPLIT the
remainder against the partial tree, never re-forward the same unit as
is. Tell every codex subagent NOT to run git at all, and check
git status --short yourself after each codex run.

ORCHESTRATOR OWNS THE BUILD OUTPUT. No worker touches web/dist. After
W4 and W5 return, run bun install --cwd web --frozen-lockfile, then
bun run --cwd web build, then cargo build --manifest-path
loom/Cargo.toml so the SPA is re-embedded. web/dist is committed, so
stage it with the rest. The after_stage check greps "u32" out of
web/dist/assets/index.js: vite pins the bundle filenames, so a skipped
rebuild leaves the tree looking clean.

SCCACHE IS OFF AND THE FIRST CARGO RUN IS COLD. sccache_usable_in
(loom/src/orchestrator/terminal/native/build_cache.rs:145-147) requires
network.allow_all_unix_sockets, which this plan deliberately does not
set, so this worktree compiles all 391 lock entries from scratch. Run
the FIRST cargo invocation through the Bash tool's run_in_background: a
foreground call cannot exceed 600000 ms and a kill at that ceiling reads
exactly like a build failure. The same applies to W1's first cargo test.
Then run the FULL gate yourself in the worktree before
loom stage complete, so the acceptance phase - capped at 300 s per
criterion (loom/src/verify/criteria/config.rs:11), unraisable from a
plan - runs warm. This stage's acceptance proves this stage's code:
fmt, clippy --all-targets, rustdoc with -D warnings (this stage adds
public doc comments, and build/clippy/test never run the rustdoc
lints), the four module filters, the maintainability target, and five
named tests run by full path. The unfiltered suite, cargo audit and
scripts/flake-check.sh belong to integration-verify alone (recorded
mistake: "The Same Suite Ran Once Per Stage, Per Check, Per Judge").
Before loom stage complete, run cargo test --manifest-path
loom/Cargo.toml --all-targets --no-fail-fast ONCE yourself so a
cross-module break surfaces here rather than one stage later; it took
57 s warm at HEAD.

SIBLING PLANS. PLAN-web-host-graft-followthrough and
IN_PROGRESS-PLAN-loop-recovery share loom/src/commands/status/web/**,
web/dist/**, loom/maintainability-baseline.txt and the knowledge tree
with this plan. If either merged after this worktree was cut: rebase,
rebuild web/dist, and re-run the full gate before completing. Never
touch config_api/request.rs or config_api/csrf.rs.

TUI ENTER. Enter opens the text editor only for Number/String keys and
steps a Bool/Enum forward (W2's brief, Task 2). This is the plan's
goal; do not accept a W2 report that kept the old Enter-edits-all.

PROJECT VALUES. An invalid [models]/[pressure] project value falls
through to the user tier exactly as the daemon does - it never fails
GET /api/config and never blocks a POST that overwrites or clears it
(W3's brief, Task 4). Reject a W3 report that made section_key
strict.

SPLIT A TEST FILE RATHER THAN GROWING IT PAST 400 LINES, AND NO
FUNCTION PAST 50. Both are gates, not style notes:
loom/tests/maintainability/scanner.rs:6-7 measures them against the
exact ledger in loom/maintainability-baseline.txt, and this stage's
acceptance runs cargo test --test maintainability so a breach lands in
the stage that caused it. Three test files are already close:
web/src/components/settings-dialog.test.tsx (380),
loom/src/commands/status/web/config_api/tests/updates.rs (366),
loom/src/user_config/tests.rs (357). Each brief names the submodule to
add instead. W2's loom/src/commands/config/tui/tests/cycling.rs is
UNCONDITIONAL, declared with mod cycling; from tui/tests.rs, not from
tui/mod.rs.

RETYPE THE POST BODIES. Every POST body in
config_api/tests/updates.rs and commands/status/web/tests/config_api.rs
carries value as a JSON string today, and nothing type-checks a string
literal inside a raw-string body - the compiler stays silent while the
assertions invert. W3's brief lists every line. The one that flips
meaning is a_malformed_body_is_rejected_without_naming_a_path
(updates.rs:289-302), which asserts 400 for
{"scope":"user","name":"update.check","value":true} - after this stage
that is the CORRECT body and returns 200.

MEMORY: record mistakes, decisions and surprises via loom memory
immediately; NEVER loom knowledge (this is an implementation stage);
NEVER Claude Code auto-memory.

#### Files Changed

No changes recorded.

#### Key Decisions

- ConfigValue::checked rejects Bool/Number against ValueKind::String with a new message '<name>: <raw:?> is not a string' rather than coercing to text *(parse never fails for String, so there is no parse wording to reuse; the brief requires every variant/kind mismatch to be rejected)*

#### Notes

- gotcha: the pre-commit markdownlint fixer rewrites any plan-prose line that wraps to start with '+ ' into a list item (inserts a blank line), leaving an unstaged edit to doc/plans/IN_PROGRESS-PLAN-typed-config-values.md:186 after every commit. Prevention: never wrap prose so a line begins with '+'; after committing, check git status and revert the hook's plan edit
- found: base commit f06b6cf6 (ci: add changelog to the release notes) added assertions for commands/changelog.md (loom/src/assets/mod.rs:54, loom/src/assets/tests.rs:197) but never committed commands/changelog.md, so the unfiltered lib suite fails 2 tests (assets::tests_table::codex_and_claude_tables_include_required_files, assets::tests::hooks_and_managed_documents_have_expected_form) on every branch cut from 81703c06. Not caused by typed-config-values; integration-verify must see commands/changelog.md committed on main (likely an untracked file in the operator's checkout)
- gotcha: codex unit w2b extracted dispatch_key/dispatch_edit_key but left ConfigTui::handle_edit_key as a dead wrapper; only clippy --all-targets -D warnings (dead_code) caught it, since scoped cargo test passed. Prevention: when a brief says 'extract X into free functions', grep for the old method afterwards
- mistake: ConfigValue::checked fell back to parse(kind, name, &self.to_string()) on a variant mismatch, so Text("false") checked against Bool became Ok(Bool(false)); the brief required every mismatch to be rejected, and only the dashboard test a_string_for_a_bool_key_is_rejected_by_checked caught it. Prevention: a 'reject on mismatch' rule needs a unit test with a mismatched value whose TEXT still parses for the target kind. Fix: mismatch arm always errs with parse's wording via shared message helpers
- gotcha: loom subagents watch exited 3 claiming codex companion job process 118 'is gone while its record says running' seconds after the forward started; the low PID points at the forwarder's sandbox PID namespace being invisible to the watcher, so treat that exit as unverified and wait for the forwarder's own LOOM-FORWARD-END report. Also: a watch naming only codex workers exits 5 'worker set does not resolve to one Claude parent UUID' - include the forwarder's claude:<agent-id> in the set
- gotcha: ConfigValue::checked re-parses the value's Display text on a variant mismatch, so a Bool/Number checked against ValueKind::String is accepted as coerced text instead of rejected; harmless while no registry key has kind String

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final verification. Verify FUNCTIONAL INTEGRATION, not just tests
passing. NEVER Claude Code auto-memory.

Use parallel subagents and skills to maximize performance.

CONTEXT: read doc/plans/PLAN-typed-config-values.md (or its
IN_PROGRESS- name), the worker briefs under
doc/plans/briefs/typed-config-values/, loom memory show --all, and
doc/loom/knowledge/INDEX.md.

BUILD & TEST, zero tolerance, nothing is "pre-existing": the full
canonical gate from CONTRIBUTING.md / loom/.githooks/pre-push (fmt,
clippy --all-targets, rustdoc with -D warnings, cargo audit, cargo test
--all-targets --no-fail-fast, scripts/flake-check.sh) and the whole web
check script. cargo audit runs offline (--no-fetch --stale): its fetch
host, github.com, is outside this sandbox, and the plan changes no
lockfile.

CODE REVIEW: spawn parallel loom-code-reviewer subagents covering
security (via Skill loom-skills with args loom-security-audit),
architecture, and test coverage. Fix ALL findings with an engineer
agent; the reviewer is read-only.

FUNCTIONAL VERIFICATION - prove each of the four surfaces reaches the
typed value, not just that it compiles:

1. CLI: with an isolated LOOM_HOME, loom/target/debug/loom config --list
   and --print render every kind, and a rejected value still names the
   key and its expected values.
2. TUI: the headless state machine cycles terminal.backend from native
   to tmux and back WITHOUT the text editor, and refuses to cycle
   context.ceiling_tokens. Enter through the key dispatcher opens the
   editor only on a Number/String key and steps a Bool/Enum.
2b. Project tier: a scratch workspace with [models]
   standard_model = "claude-opus-5", then with standard_model = 42,
   loads GET /api/config, shows the daemon's resolved model as
   effective, and accepts a POST that overwrites and then clears the
   key.
3. Web wire: the regenerated web/src/api/fixtures/config.json carries a
   JSON boolean for update.check, a JSON number for both Number keys,
   and a JSON string for every enum key - and the page's zod schema
   parses it.
4. ValueKind::String: confirm the variant is exercised by a real test on
   both sides, not merely defined. A variant no test drives is the
   failure this check exists to catch.

Also confirm web/dist was rebuilt and committed after the last web/src
edit: loom/build/assets.rs embeds it, so a stale dist ships an old SPA
against the new wire.

WIRING TESTS ARE CAPPED AT 30 s
(loom/src/verify/goal_backward/wiring_tests.rs:13) and no plan key can
widen that. Both entries run AFTER the acceptance phase, so the lib test
binary is already built - never make a wiring test the first build of a
target. Both filters are deliberately narrow, and a cargo filter that
matches zero tests exits 0, which is why stage 1's after_stage greps for
the test function names by hand.

Record discoveries to loom memory for knowledge-distill, including any
knowledge file contradicted by the tree:
loom memory note "stale-knowledge: ...".

#### Files Changed

No changes recorded.

#### Key Decisions

- kept EnumSelect's synthesized option for an off-list current value although the ConfigValue Enum invariant makes it unreachable today *(the W5 brief settled it (w5-web-controls.md:217-221, :360): the comment and behaviour stay as a guard for a registry that drops a variant; the architecture reviewer flagged it Low and deferred to the plan)*
- left configValueSchema kind-agnostic (z.union of boolean/number/string for every entry) instead of refining each value against kind.type *(the server is the sole validator and the SPA ships in the same binary as the registry; the plan's functional check only requires the schema to parse a correctly typed payload (IN_PROGRESS-PLAN-typed-config-values.md:1020-1023), and formatValue deliberately tolerates a mismatch (settings-model.test.ts:214-216). The test-coverage reviewer flagged it as a design question, not a defect)*

#### Notes

- mistake: PLAN-typed-config-values' integration-verify wiring entry grepped 'ConfigValue' in loom/src/commands/config/mod.rs, but the CLI reads the typed value through config.value_of(spec) (mod.rs:65, :90) with the (ConfigValue, Origin) return type inferred, so the name never appears and goal-backward verification failed after every acceptance criterion passed. Why: a wiring pattern named a type the consumer only infers. Prevention: a wiring pattern must be a literal the source file spells; dry-run each wiring regex with rg against the post-change tree AND the pre-change tree (it must match after and fail before) when writing the plan. Fix: operator amend to the value_of signature in user_config/render.rs:17. Also: loom stage amend cannot run inside a stage session (EROFS creating .loom/work/plan_versions), so only the operator can repair a wiring entry; dispute-criteria covers acceptance indexes only
- found: the 2 failing assets tests (commands/changelog.md missing) were fixed by the operator committing commands/changelog.md and scripts/changelog-section.sh on main (d4078f31, amended from 4f992da8). The IV branch had fast-forwarded to the pre-amend commit, so it was moved with git reset --keep main (no commits of its own, uncommitted fixes kept). Gotcha: .github/workflows/release.yml:247 calls scripts/changelog-section.sh inside an if, so a missing script silently published release notes without the changelog section; rg skips .github by default, so search it with rg --hidden
- mistake: the IV orchestrator's functional-verifier brief demanded that a fixture with update.check's user value as the string "true" fail configResponseSchema.parse, so the verifier reported a FAIL. Why: the plan specified configValueSchema as a kind-agnostic z.union (w4-web-model.md:44) and only requires the schema to parse a correctly typed payload. Prevention: before writing a negative expectation into a verification brief, check the plan or brief states that behaviour; a verifier cannot tell a brief's invention from the contract
- found: loom config has no non-interactive unset. user_config::write::unset (write.rs:61) is reached only from the dashboard's POST value:null path (config_api/apply.rs:33); the CLI (cli/types_config.rs) and the TUI never call it, so an operator can only clear a user-tier key by editing ~/.loom/config.toml or from the settings page. Also: a bare negative value needs '--' (loom config -k context.ceiling_tokens -- -1) because clap reads -1 as a flag
- found/gotcha: full cargo test suite fails 2 tests unrelated to typed-config-values: assets::tests_table::codex_and_claude_tables_include_required_files (loom/src/assets/mod.rs:54) and assets::tests::hooks_and_managed_documents_have_expected_form (loom/src/assets/tests.rs:197). Both assert commands/changelog.md is a registered/installed asset, but the file was never added. Introduced by commit f06b6cf6 'ci: add changelog to the release notes' which added the assertions to mod.rs/tests.rs without adding commands/changelog.md itself or a CLAUDE_COMMANDS table entry. Deterministic (reran each 3x, same panic every time), not caused by this plan's changes; git status is clean on this worktree.
- found/gotcha: TUI tests cannot drive ConfigState::cycle on a ValueKind::String row: rows are built only from the static KEYS registry (no key has kind String) and ConfigState's fields are private to tui::state, which tui::tests::cycling cannot reach as a sibling module. String's refuse-to-cycle behaviour is covered only by sharing the Number arm in cycle plus the opens_editor(&ValueKind::String) assertion; a test needs a registered String key or a pub(super) test constructor
- found/gotcha: a backgrounded process (loom status --web &) started in one Bash tool call is unreachable from a later Bash call — each Bash invocation gets its own sandboxed process namespace, so ps/curl in a follow-up call see nothing and curl gets connection refused. Prevention: start the server, wait for its port, curl it, and kill it all within ONE Bash command.
- found/gotcha: the config TUI renders a row's ConfigValue::Text straight into a ratatui Cell with no control-character stripping (tui/render.rs value_cell). Harmless today because no KEYS entry uses ValueKind::String and the TUI edits only the operator's own ~/.loom/config.toml; when the first String-kind key is registered, strip control characters before building the Cell or accept the self-inflicted risk explicitly
- gotcha: loom subagents watch rejects the agent id the Agent tool returns when the spawn passes a name (form name@session-<8hex>): WorkerSpec::from_str calls forward_receipt::is_safe_id, which disallows '@', so the watch exits 5 'unknown: worker id is empty or unsafe' before binding anything. The same run under the Bash tool's run_in_background was reported as exit 0 by the task notification, while a foreground run shows exit=5. Prevention: spawn watched Claude workers without a name, or wait on the harness completion notifications when the ids carry '@'; always read the watch output, never trust the background exit code alone

## Open Questions

No open questions.
