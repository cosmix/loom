# Plan: Adopt Best Practices from Everything Claude Code

## Context

The [Everything Claude Code](https://github.com/affaan-m/everything-claude-code) (ECC) project represents 10+ months of battle-tested Claude Code workflows with 30+ agents, 135+ skills, 60+ commands, and automated hooks. This plan identifies which ECC patterns would meaningfully improve loom's development workflow, filtering out what loom already handles better.

**Key insight:** Loom's orchestration, signal system, knowledge management, and verification are already more sophisticated than ECC's equivalents. The biggest wins come from ECC's **enforcement hooks** (catching issues at edit-time rather than commit-time) and **Rust-specific quality automation** that loom currently lacks.

---

## What Loom Already Does Better Than ECC

| Area                 | Loom                                                               | ECC                               |
| -------------------- | ------------------------------------------------------------------ | --------------------------------- |
| Agent orchestration  | DAG-based parallel stages with progressive merge                   | Manual tmux/worktree coordination |
| Knowledge management | 7-file curated system + memory/knowledge separation                | Flat session persistence          |
| Agent instructions   | KV-cache efficient 4-layer Manus signals                           | Static CLAUDE.md                  |
| Verification         | 4-layer goal-backward (artifacts, wiring, wiring-tests, dead-code) | Manual /verify command            |
| Sandbox              | Plan+stage level merged config with path isolation                 | Generic Docker patterns           |
| Skill matching       | Dual-layer (signal-time + prompt-time) with scoring                | Manual skill loading              |

**These areas need no changes.**

---

## Recommendations

### Tier 1: High Value (address real gaps)

#### 1.1 Post-Edit Rust Quality Hooks

**Problem:** Formatting and lint issues are only caught at commit time. Agents write unformatted code, accumulate clippy warnings, then face a wall of errors when committing.

**Solution:** Add PostToolUse hooks for Edit/Write on `.rs` files:

- `cargo fmt` (sync, fast) - auto-format after every Rust edit
- `cargo clippy --quiet` (async, background) - lint check after edits
- `cargo check --quiet` (async, background) - verify compilation

**ECC equivalent:** `post-edit-format.js` (prettier), `post-edit-typecheck.js` (tsc)

**Files to create:**

- `loom-hooks/post-edit-rustfmt.sh` - Format on Edit/Write of .rs files
- `loom-hooks/post-edit-clippy.sh` - Async clippy after .rs edits
- Update `.claude/settings.json` with new PostToolUse hooks

#### 1.2 Config Protection Hook

**Problem:** Agents sometimes weaken linter/formatter configs instead of fixing the code. This is a documented anti-pattern across the industry.

**Solution:** PreToolUse hook on Edit/Write that blocks modifications to quality config files: `.rustfmt.toml`, `clippy.toml`, `deny.toml`, `rust-toolchain.toml`, and cargo workspace-level lint settings.

**ECC equivalent:** `config-protection.js` - blocks linter/formatter config edits

**Files to create:**

- `loom-hooks/config-protection.sh`
- Update `.claude/settings.json`

#### 1.3 File Size Enforcement Hook

**Problem:** Loom's CLAUDE.md specifies 400-line file limit and 50-line function limit, but these are advisory. Agents can still create oversized files.

**Solution:** PreToolUse hook on Write that counts lines and blocks files exceeding 400 lines. Warn (don't block) on Edit if the edit would push a file over the limit.

**ECC equivalent:** File size guard pattern (block >800 lines)

**Files to create:**

- `loom-hooks/file-size-guard.sh`
- Update `.claude/settings.json`

#### 1.4 Debug Print Audit Hook

**Problem:** `concerns.md` specifically documents that `eprintln!` statements with "Debug:" prefix exist in production code. This is the Rust equivalent of ECC's console.log problem.

**Solution:** PostToolUse on Edit + Stop hook to detect `eprintln!("Debug` and `dbg!()` in modified `.rs` files. Warn but don't block (some debug prints are intentional during development).

**ECC equivalent:** `post-edit-console-warn.js`, `check-console-log.js`

**Files to create:**

- `loom-hooks/debug-print-audit.sh`
- Update `.claude/settings.json`

#### 1.5 `--no-verify` Blocker

**Problem:** Agents can bypass pre-commit hooks with `--no-verify`, defeating all commit-time quality gates.

**Solution:** PreToolUse hook on Bash that detects `--no-verify` and `--no-gpg-sign` flags in git commands and blocks them (exit 2).

**ECC equivalent:** `block-no-verify` (npm package)

**Files to create:**

- `loom-hooks/block-no-verify.sh`
- Update `.claude/settings.json`

---

### Tier 2: Medium Value (enhance existing patterns)

#### 2.1 Hook Profile System

**Problem:** Loom's hooks are all-or-nothing. During rapid prototyping you might want fewer checks; during release prep you want strict enforcement.

**Solution:** Environment variable `LOOM_HOOK_PROFILE` with levels:

- `minimal` - Only safety-critical hooks (worktree isolation, commit guard)
- `standard` (default) - All current hooks
- `strict` - Standard + post-edit quality + file size guard + debug audit

Each hook script checks the profile and exits early if not in its level.

**ECC equivalent:** `ECC_HOOK_PROFILE` + `ECC_DISABLED_HOOKS` env vars, `run-with-flags.js` wrapper

**Files to create:**

- `loom-hooks/lib/hook-profile.sh` - Shared profile checker
- Update existing hooks to source the profile checker

#### 2.2 Dependency Security in Pre-Commit

**Problem:** Known CVEs in dependencies aren't caught until CI (if at all).

**Solution:** Add `cargo audit --quiet` to the pre-commit hook chain. Only runs when `Cargo.toml` or `Cargo.lock` has changed (to keep it fast).

**ECC equivalent:** Pre-commit quality check that includes secret scanning

**Files to modify:**

- `loom-hooks/git-pre-commit-hook.sh` - Add cargo audit step

#### 2.3 Rust-Specific Rules File

**Problem:** Rust coding conventions are scattered across CLAUDE.md and knowledge files. ECC's approach of dedicated language rules files provides more targeted guidance.

**Solution:** Create `.claude/rules/rust.md` with Rust-specific conventions that complement the existing CLAUDE.md. Cover:

- Ownership/borrowing patterns (prefer `&str` over `String` in params)
- Error handling (anyhow + with_context)
- Testing patterns (tempfile, serial_test)
- Unsafe code policy (SAFETY: comments mandatory)
- Iterator preferences

**ECC equivalent:** `rules/rust/coding-style.md`, `rules/rust/testing.md`, `rules/rust/security.md`

**Files to create:**

- `.claude/rules/rust.md`

---

### Tier 3: Lower Value (nice to have, defer)

#### 3.1 Session-End Pattern Extraction

Lighter version of ECC's continuous learning - prompt `loom memory` recording at session end.

#### 3.2 Desktop Notifications

OS notifications when long-running loom stages complete.

#### 3.3 Cost/Token Tracking

Per-session token usage metrics feeding into loom's monitoring.

#### 3.4 Governance/Audit Capture

Structured logging of security events for enterprise compliance.

---

## What NOT to Adopt (and Why)

| ECC Feature                         | Why Skip                                                                                                        |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| Agent system (30+ agents)           | Loom's `loom-senior-software-engineer` / `loom-software-engineer` / `loom-code-reviewer` are more context-aware |
| Skill system wholesale              | Loom already has 60+ skills with signal injection and dual-layer matching                                       |
| Slash commands (/tdd, /plan, etc.)  | Loom's CLI commands are built into the Rust binary                                                              |
| Session persistence (/save-session) | Loom's memory + handoff + knowledge system is more structured                                                   |
| TDD workflow                        | Not a natural fit for loom's Rust development patterns                                                          |
| Model routing                       | Already handled in signal generation and CLAUDE.md agent selection                                              |
| Continuous-learning-v2 observer     | Complex Node.js system, overkill. Lighter hook approach is better                                               |
| MCP health checks                   | Loom doesn't use MCPs heavily                                                                                   |
| Multi-plan/multi-workflow           | Loom's DAG orchestration already handles this natively                                                          |

---

## Implementation Approach

**Phase 1 (Tier 1 hooks):** Create the 5 new hook scripts and update settings.json. Each hook is independent and can be developed and tested in isolation. Estimated: ~200 lines of shell scripts total.

**Phase 2 (Tier 2 enhancements):** Hook profile system, cargo audit integration, Rust rules file. These build on Phase 1 hooks. Estimated: ~150 lines of shell + 1 markdown file.

**Phase 3 (Tier 3, if desired):** Defer to separate plans based on priority.

---

## Verification

- Run each hook manually with simulated tool input (JSON on stdin)
- Edit a .rs file and verify `cargo fmt` runs automatically
- Try to edit `.rustfmt.toml` and verify it's blocked
- Write a >400 line file and verify it's blocked
- Try `git commit --no-verify` and verify it's blocked
- Add a `dbg!()` to a .rs file and verify the warning
- Set `LOOM_HOOK_PROFILE=minimal` and verify only safety hooks run
- Run full test suite: `cd loom && cargo test`

---

## Critical Files

**Existing hooks to reference:**

- `loom-hooks/prefer-modern-tools.sh` - Pattern for PreToolUse matching
- `loom-hooks/commit-filter.sh` - Pattern for Bash command inspection
- `loom-hooks/worktree-isolation.sh` - Pattern for Edit/Write file matching
- `loom-hooks/post-tool-use.sh` - Pattern for PostToolUse processing

**Settings to modify:**

- `.claude/settings.json` - Hook registration

**Knowledge to update:**

- `doc/loom/knowledge/patterns.md` - Hook conventions section
