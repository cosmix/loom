# Code Review: Reworked `loom knowledge bootstrap`

**Plan:** PLAN-knowledge-bootstrap-command | **Generated:** 2026-09-22 21:36 UTC

## Summary

## Overview

## Changes by Stage

### Implement loom knowledge bootstrap (implement-bootstrap)

**Status:** completed  
**Purpose:** Re-add `loom knowledge bootstrap` on top of the current knowledge system and
source graph. The plan prose's "Design decisions (settled)" table is binding;
YAML is authoritative where prose and YAML differ.
Use parallel subagents and skills to maximize performance.

FOUNDATION (main agent, before wave 1, under 20 lines):

  1. Create loom/src/commands/knowledge/bootstrap/mod.rs containing only:
       mod clusters;
       mod receipt;
       #[cfg(test)]
       #[path = "tests_clusters.rs"]
       mod tests_clusters;
       #[cfg(test)]
       #[path = "tests_receipt.rs"]
       mod tests_receipt;
     Use this three-line form, which rustfmt keeps (precedent
     loom/src/commands/knowledge/mod.rs:177-191), not the one-line form.
  2. Also create clusters.rs, receipt.rs, tests_clusters.rs and tests_receipt.rs
     in that directory, each containing only one `//!` module doc line (valid
     Rust, not a stub function). Without them every compile, including W1's
     scoped test run, fails with E0583 until W2 finishes.
  3. Add `pub mod bootstrap;` to loom/src/commands/knowledge/mod.rs next to the
     other module declarations.
  Run `cargo build` once; it must be green before wave 1.

WAVE 1: spawn W1 and W2 BY AGENT TYPE in ONE message. WAVE 2: after both
return and the main agent's gate (cargo build, cargo test --lib, cargo clippy
--all-targets -- -D warnings, cargo fmt --check) is green, spawn W3. Each
brief runs ONE scoped proof command; the main agent runs the full gate after
each wave. Territories are DISJOINT. Workers
NEVER spawn subagents. Each gets the fixed prompt plus
"Your brief: <path>. Read it in full before anything else."

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| ------ | ---- | ---- | ----------- | -------------- | ---------- |
| W1 | Session driver + pressure rewiring | sonnet | loom/src/claude.rs, loom/src/claude/session.rs, loom/src/commands/pressure/spawn.rs, loom/src/commands/pressure/paths.rs, loom/src/commands/pressure/mod.rs, loom/src/commands/pressure/tests.rs | loom/src/lib.rs (read-only) | doc/plans/briefs/knowledge-bootstrap-command/implement-bootstrap/w1-session-driver.md |
| W2 | Clusters, digests, fan-in, receipt | sonnet | loom/src/commands/knowledge/bootstrap/clusters.rs, loom/src/commands/knowledge/bootstrap/receipt.rs, loom/src/commands/knowledge/bootstrap/tests_clusters.rs, loom/src/commands/knowledge/bootstrap/tests_receipt.rs | loom/src/context/graph_store/mod.rs (read-only) | doc/plans/briefs/knowledge-bootstrap-command/implement-bootstrap/w2-clusters-receipt.md |
| W3 | Command, prompts, CLI, integration test | opus | loom/src/commands/knowledge/bootstrap/mod.rs, loom/src/commands/knowledge/bootstrap/prompt.rs, loom/src/commands/knowledge/bootstrap/graph.rs, loom/src/commands/knowledge/bootstrap/tests.rs, loom/src/commands/knowledge/mod.rs, loom/src/commands/knowledge/sync.rs, loom/src/cli/mod.rs, loom/src/cli/types_memory.rs, loom/src/cli/dispatch.rs, loom/src/context/retrieve.rs, loom/tests/integration/mod.rs, loom/tests/integration/knowledge_bootstrap.rs, loom/tests/integration/knowledge_bootstrap_support.rs, loom/src/completions/dynamic/tests/tests_commands.rs | W1/W2 APIs (read-only) | doc/plans/briefs/knowledge-bootstrap-command/implement-bootstrap/w3-command.md |

W3 is opus because it joins six seams (driver, graph, catalog refresh,
receipt, CLI, fake-claude integration test) and writes the session prompts.
CROSS-PLAN: before starting, confirm dispatch_knowledge is still in
src/cli/dispatch.rs and cli/mod.rs still re-exports AnnotateArgs; if a
sibling plan moved either, re-point this stage's checks first (see the
Cross-plan row and BLOCKING sibling amendments in the plan prose).
W1/W2 are fully specified by their briefs.

VERIFY (main agent, after wave 2): run every acceptance command below. On a
failure, re-brief the owning worker type with the failing output; a second
failure on the same issue escalates one tier (sonnet -> opus -> fable).

MEMORY: record mistakes/decisions/surprises via loom memory immediately,
subagents too; NEVER loom knowledge (implementation stage); NEVER Claude Code
auto-memory. If a knowledge file contradicts the tree:
loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y".

#### Files Changed

- claude/session.rs remove_if_exists is now the crate's single NotFound-tolerant delete; pressure::paths::delete_file removed, bootstrap remove_brief wraps it with warn-only semantics

#### Key Decisions

- knowledge bootstrap execute is pub(crate), not pub as the W3 brief wrote *(BootstrapArgs is only pub(crate)-reachable (cli/mod.rs re-export from private types_memory); a pub fn taking it trips private_interfaces under clippy -D warnings. Precedent: knowledge::annotate::annotate is pub(crate).)*
- bootstrap finalize refresh and catalog count are best-effort warnings *(The session already ran; a failed cache rebuild after it must not cost the receipt of a completed session. Pre-session refresh still fails hard.)*
- kept guard_not_in_stage's inlined LOOM_STAGE_ID check instead of reusing hook::target::non_empty_env *(commands/hook/mod.rs:14 declares 'mod target;' private; reuse needs widening a module outside this stage's files-to-modify. Candidate follow-up: pub(crate) mod target or re-export non_empty_env)*

#### Notes

- mistake: stage gate (build/test/clippy/fmt) passed but the pre-commit hook's rustdoc -D warnings rejected W3's private intra-doc links in bootstrap/mod.rs module docs. Why: the stage gate omits cargo doc. Prevention: add RUSTDOCFLAGS='-D warnings' cargo doc --no-deps to the main-agent gate when doc comments change
- found/gotcha: commands::hook::target::non_empty_env is pub(crate) but its module is private (commands/hook/mod.rs:14 'mod target;'), so knowledge bootstrap inlines the same LOOM_STAGE_ID test instead of calling it
- mistake: wrote a shell script containing "## heading" inside r#"..."# in tests/integration/knowledge_bootstrap_support.rs. Failed because the sequence quote-hash ends an r# raw string early (and quote-hash-hash ends r##). Prevention: before choosing a raw-string delimiter, check the body for a quote followed by that many #; markdown headings in quotes need r###. Fix: r###"..."###
- gotcha: both wave-1 sonnet workers left rustfmt diffs (long use-lists, match arms) since the no-verify rule bars them from cargo fmt; main agent ran cargo fmt after the wave. Prevention: budget a cargo fmt pass in the orchestrator gate after every wave
- found: three prior sessions (handoffs 001-003) ended with foundation files written but no wave spawned; resumed from foundation, cargo build green

### Integration Verification (integration-verify)

**Status:** completed  
**Purpose:** Final verification of the reworked `loom knowledge bootstrap`. Verify
FUNCTIONAL INTEGRATION, not just tests passing. NEVER Claude Code auto-memory.
Use parallel subagents and skills to maximize performance.
CONTEXT: read doc/plans/PLAN-knowledge-bootstrap-command.md (Design decisions
table), loom memory show --all, and the INDEX.md sections for
architecture/knowledge-hierarchy, architecture/source-graph,
mistakes/test-concurrency-and-fixtures, mistakes/detached-spawn-in-tests.
BUILD & TEST (zero tolerance; fix ALL warnings/errors): the acceptance list.
CODE REVIEW: spawn three loom-code-reviewer subagents in ONE message:
  R1 security: the claude argv (variadic --disallowedTools must not swallow
     the positional prompt), marker/brief paths, the .loom/.gitignore write
     conditions, receipt JSON parsing of a committed (untrusted) file, the
     receipt load (Ok(None) only on symlink_metadata NotFound, otherwise
     crate::fs::safe_read::read_to_string_bounded with a 1 MiB cap, so a
     symlinked or oversized receipt is an error), the receipt write
     (crate::fs::locking::locked_write, never atomic_write_locked), the
     LOOM_STAGE_ID guard, the run lock (non-blocking, held for the whole
     run, never the knowledge-directory lock across the session), and
     require_snapshot refusing SnapshotAction::Unavailable before any spawn.
  R2 architecture/maintainability: driver extraction left no duplicate in
     pressure; every file at most 400 lines, every function at most 50; no dead code.
  R3 tests: the fake-claude integration test fails if bootstrap stops
     spawning, stops writing the receipt, or --refresh re-spawns when nothing
     changed; a removal-only change still spawns (test 8); touch-then-exit
     writes the receipt (test 9); refresh is current after commit and in a
     fresh clone (test 10); no test leaves a process behind (exec sleep in
     the fake).
Fix every finding with a loom-software-engineer subagent (opus if it failed once).
FUNCTIONAL SMOKE (main agent): in a scratch repo under
"${TMPDIR:-/tmp}", git init, commit two small .rs files, then run the built
binary with every tests/integration/helpers.rs RELAY_ENV_VARS_TO_CLEAR
variable removed and a scratch LOOM_HOME (resolve the manifest path
before cd-ing into the scratch repo):
  env -u LOOM_SESSION_ID -u LOOM_STAGE_ID -u LOOM_WORK_DIR -u LOOM_WORKTREE_PATH -u LOOM_MAIN_AGENT_PID -u LOOM_SESSION_TYPE -u LOOM_MERGE_SESSION -u LOOM_SCRATCH_DIR -u LOOM_BIN -u LOOM_HOOK_PATH -u LOOM_HOOK_CONTEXT -u LOOM_CONTROL_BROKER LOOM_HOME=<scratch dir containing config.toml with "[update]\ncheck = false"> cargo run --quiet --manifest-path <worktree>/loom/Cargo.toml -- knowledge bootstrap ...
with --structural-only, then --dry-run, then --refresh --dry-run. NEVER a
bare --refresh: after --structural-only there is no receipt, so it would
spawn the real claude. Confirm coverage and cluster lines (no
doc/loom/knowledge path in the table), doc/loom/knowledge/INDEX.md,
.loom/.gitignore, and that git status --porcelain lists nothing under
.loom/. Check the scratch-repo files with Bash `test -f`: the worktree
file guard blocks the Read tool outside the worktree
(mistakes/sandbox-and-settings.md:277-281).
Record discoveries to loom memory for knowledge-distill, including any
stale knowledge: loom memory note "stale-knowledge: ...".

#### Files Changed

No changes recorded.

#### Key Decisions

- accepted: cluster ids and file paths enter the bootstrap brief verbatim (prompt-injection surface via crafted directory names); no escaping added *(The autonomous session reads repository file contents anyway, which are an equal injection surface; the plan's Write-restriction row already accepts that Bash can write. Escaping names would not narrow the surface)*
- kept bootstrap completion_instruction as a separate copy of pressure's wording, no shared template *(Pressure's prompt text is pinned by pressure/tests.rs and the plan keeps pressure's behaviour unchanged; the two differ in command name and completion condition)*

#### Notes

- concern: in a no-network stage sandbox the pre-commit hook's 'Linting markdown files...' step tries registry.npmjs.org:443, is denied, and the commit still succeeds silently. A stage committing .md files would get no markdown lint. Seen on IV commits da7c5439/5ae15790 (no .md staged, so nothing skipped here)
- gotcha: shrinking a file listed in loom/maintainability-baseline.txt below its limit fails tests/maintainability.rs with 'stale entry ... no longer violates its limit; remove it'. Moving fs/locking.rs's tests to fs/locking/tests.rs required deleting its baseline line in the same change. Prevention: after any file split, rg the path in maintainability-baseline.txt
- mistake: fix subagent's load_oversized_receipt_gives_err wrote 'a' x (cap+1), which is invalid JSON, so Receipt::load errored on parsing and the test passed with the size cap removed. Why: asserted only is_err(). Prevention: a negative test must isolate its cause; feed input valid in every other respect (here valid receipt JSON padded with whitespace past MAX_RECEIPT_BYTES) and assert the specific error text ('byte verification limit'). Fixed in IV
- found: fs/locking.rs atomic_write opened <path>.tmp without O_NOFOLLOW, so a TRACKED symlink named <file>.tmp in a cloned repo (e.g. doc/loom/knowledge/.bootstrap-receipt.json.tmp) redirected every locked_write/atomic_write_locked write outside the repo. IV added O_NOFOLLOW (fail closed). Directory components of the target path are still followed crate-wide (a tracked symlinked doc/loom/knowledge or .loom is written through by other commands such as loom map's overlay)
- concern: knowledge bootstrap --dry-run prints the claude argv with argv.join(" ") (no shell quoting), so the multi-line --append-system-prompt value and the positional prompt run together and the line cannot be pasted into a shell. It copies pressure's render_dry_run_step, whose output tests pin; a shared quoting helper would fix both
- gotcha: the integration-verify signal's smoke command unsets LOOM_MAIN_AGENT_PID, which commit-filter.sh blocks in an orchestrator Bash call. knowledge bootstrap never reads that variable, so the smoke ran with every other RELAY_ENV_VARS_TO_CLEAR variable unset and passed (--structural-only, --dry-run, --refresh --dry-run). Prevention: plan authors drop LOOM_MAIN_AGENT_PID from main-agent smoke commands; wiring_tests run by loom check are unaffected

## Open Questions

No open questions.
