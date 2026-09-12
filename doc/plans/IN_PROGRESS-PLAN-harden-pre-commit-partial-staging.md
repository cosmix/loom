# Plan: Harden Pre-Commit Partial Staging

Created: 2026-09-12

## Overview

Retire the actionable remainder of `PLAN-adopt-ecc-best-practices.md` in favor of one
current enforcement fix: prevent Loom's repository pre-commit formatter from replacing a
partially staged file with its complete working-tree contents. Add a black-box regression to CI
so the index-preservation contract cannot silently regress.

The broader ECC recommendations are omitted because current code has superseded them or because
they lack a concrete defect. In particular, the maintainability gate already enforces file and
function limits, pre-push already runs Cargo audit, Rust conventions live in curated knowledge,
and the current Rust source contains no `dbg!()` or `eprintln!("Debug:` matches. A global
`git commit --no-verify` blocker is also omitted: project knowledge documents it as an
operator-authorized recovery route when the hook itself cannot safely run.

## Goals

- Detect every path that is both staged and modified again in the working tree before any
  formatter, linter, or `git add` command runs.
- Abort without changing the index or working tree and name every conflicting path.
- Preserve the existing auto-format and re-stage behavior when all staged paths are fully staged.
- Exercise the real hook in a temporary Git repository with fake build tools and run the
  regression in CI.
- Keep the production hook surface, generated Claude/Codex settings, stage verification,
  notification, telemetry, and dependency policy unchanged.

## Grounded Current State

- `loom/.githooks/pre-commit` captures staged filenames, runs repository-wide Rust and Markdown
  formatters, then unconditionally runs `git add` on every originally staged path.
- A minimal probe against the current hook exited 0, changed the index, and made the index equal
  to a working tree containing an intentionally unstaged line.
- Git currently resolves `core.hooksPath` to `loom/.githooks`, so this is the hook used by commits
  in this repository.
- `doc/loom/knowledge/mistakes/testing-and-lint.md`, section "The Pre-Commit Hook Re-Adds Every
  Staged File, So Partial Staging Is Silently Undone", records the same incident and the manual
  `--no-verify` recovery procedure.
- `loom knowledge sync` reported the catalog and source graph current, so this plan skips
  knowledge-bootstrap.

## Verified Baseline

Observed on the current checkout before authoring this plan:

- `loom repair` (dry run): workspace healthy.
- `./scripts/check-hook-syntax.sh`: 104 shell scripts parsed cleanly.
- `bash loom-hooks/tests/run-all.sh`: 63 passed, 0 failed.
- `cargo build --all-targets`, `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, and rustdoc with warnings denied: passed.
- `cargo audit --no-fetch -d "$HOME/.cargo/advisory-db"`: 387 dependencies scanned against
  1,243 loaded advisories, exit 0.
- `cargo test --all-targets --no-fail-fast`, with temporary fixtures under system `/tmp` and Loom
  session/git override variables removed: 4,549 passed, 0 failed, 6 ignored.
- `./scripts/flake-check.sh`: four filters, 20 loaded runs each, passed.
- `loom knowledge check --strict`: 0 issues; review-only reference notes remain non-blocking.

The exhaustive suite uses the dedicated sandbox grant `/tmp/loom-pre-commit-plan`: putting
`TMPDIR` below this repository causes tests that intentionally create a non-Git directory to
discover the parent checkout and exercise the wrong branch. Every stage remains sandboxed. Cargo
commands unset `RUSTC_WRAPPER` because the sandbox denies the sccache Unix socket, and tests use
their existing capability probes when the sandbox denies process or socket operations.

## Scope Decisions

| ECC proposal | Decision |
| --- | --- |
| Post-edit fmt/clippy/check | Superseded by staged and stage-level verification; repeated Cargo work would race and overrun edit loops. |
| Config protection | Omitted; only `loom/deny.toml` currently matches the proposed set, and legitimate policy edits need to remain possible. |
| File-size hook | Superseded by `cargo test --test maintainability` and the shrinking-only ledger. |
| Debug-print audit | Omitted; its motivating production literals are absent, while normal `eprintln!` is intentional CLI output. |
| Git `--no-verify` blocker | Omitted; it cannot distinguish agent evasion from explicit operator recovery. `--no-gpg-sign` has no verification role. |
| Hook profiles | Omitted; an environment-selected minimal profile would weaken unconditional guards. |
| Cargo audit at pre-commit | Superseded by the blocking pre-push and CI audit gates. |
| Rust rules file | Superseded by `doc/loom/knowledge/conventions/code-style-and-structure.md`. |
| Memory, tokens, notifications, governance | Existing systems partially cover these; the remaining product-policy questions have no acceptance contract in the ECC proposal. |

## Execution Diagram

```mermaid
graph LR
    harden-pre-commit --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. Harden Pre-Commit

This is the plan's only implementation stage. Stage-necessity question Q3 requires one named
behavioral checkpoint before integration verification; Q1, Q2, and Q4 do not justify splitting
the work into additional worktrees. Two Terra workers operate in parallel on disjoint files: one
changes the hook, and one builds its black-box regression and CI consumer.

The hook must inspect the staged set with NUL-safe filename handling. For each staged added,
copied, or modified path, compare the working tree to the index. Exit nonzero before `cargo fmt`,
Markdown lint, or any re-staging when at least one path differs; distinguish Git's ordinary
"different" exit from an actual comparison error. Report all conflicting paths and explain that
the user must fully stage, unstage, or explicitly perform the documented manual-check recovery.
With no partially staged path, retain today's formatting, maintainability, conditional rustdoc,
and re-stage flow.

The regression must invoke the checked-in hook, not a copied reimplementation. It creates an
isolated Git repository and fake `cargo`/`bunx` executables, then proves both sides: a partially
staged Rust file fails before either tool runs and leaves index/worktree bytes unchanged; a fully
staged Rust file reaches the fake tools and succeeds. Include a filename containing spaces and an
unstaged-only control path so line-based filename handling and over-broad detection fail the test.

### 2. Integration Verification

Run the repository's full shell and Rust gates with zero tolerance, then review the final diff for
index safety, shell portability, CI reachability, and test discrimination. The black-box fixture
must prove the actual hook is selected and must fail if the partial-staging guard is removed or
moved below a mutating command.

### 3. Knowledge Distillation

Replace the existing partial-staging incident section with the shipped invariant and regression
path, preserve the explicit operator-recovery distinction, resolve all plan memories, and run the
strict knowledge and memory checks. No user-facing README or CONTRIBUTING change is expected
because this is repository-maintainer behavior already documented in project knowledge.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 1
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read:
        - "~/.ssh/**"
        - "~/.aws/**"
        - "~/.config/gcloud/**"
        - "~/.gnupg/**"
        - ".loom/work/admin.token"
        - ".loom/work/user.token"
      allow_write:
        - "loom/.githooks/pre-commit"
        - "scripts/test-pre-commit-partial-staging.sh"
        - ".github/workflows/ci.yml"
        - "target/**"
        - "loom/target/**"
        - "/tmp/loom-pre-commit-plan"
    network:
      allowed_domains: []
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: harden-pre-commit
      name: "Harden Pre-Commit Partial Staging"
      stage_type: standard
      model: "opus"
      reasoning_effort: "high"
      implementers: ["codex"]
      subagent_timeout_secs: 900
      description: |
        Prevent the repository pre-commit hook from replacing a partially staged
        index entry with the complete working-tree file, and pin the behavior in CI.
        Use parallel subagents and skills to maximize performance.

        STAGE NECESSITY: Q3 requires this single behavioral implementation checkpoint
        before integration verification. Q1, Q2, and Q4 do not justify more stages;
        the two file territories are independent and fit one session.

        Spawn both loom-codex-forwarder workers in the foreground, in one message,
        with --model gpt-5.6-terra, --effort xhigh, a 600000 ms Bash timeout,
        and the fixed prompt: own only the listed files, read the brief first, use
        apply_patch, never run git, never spawn subagents, and report changed files
        plus the one permitted narrow check. If a Terra attempt fails once against
        the stated acceptance, escalate that worker once to gpt-5.6-sol at xhigh
        with the failure evidence attached.

        Territories are DISJOINT. Workers NEVER spawn subagents. The orchestrator
        runs all combined verification after both workers return.

        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | ------ | ---- | ---- | ----------- | -------------- | ---------- |
        | W1 | Partial-staging guard | gpt-5.6-terra | loom/.githooks/pre-commit | doc/loom/knowledge/mistakes/testing-and-lint.md (read-only) | doc/plans/briefs/harden-pre-commit-partial-staging/harden-pre-commit/w1-hook.md |
        | W2 | Black-box regression and CI wiring | gpt-5.6-terra | scripts/test-pre-commit-partial-staging.sh; .github/workflows/ci.yml | loom/.githooks/pre-commit (read-only) | doc/plans/briefs/harden-pre-commit-partial-staging/harden-pre-commit/w2-regression.md |

        FOUNDATION CONTRACT: before spawning, state that W1's hook exits nonzero
        before every mutating tool when any staged ACM path also differs between
        index and working tree; W2 tests exactly that public shell behavior and
        does not depend on W1's internal helper names.

        MEMORY: record mistakes, decisions, and surprises immediately with loom
        memory; subagents include any such findings in their reports. NEVER use
        loom knowledge or editor auto-memory in this implementation stage.
      dependencies: []
      before_stage:
        - command: 'rg -qF ''git add "$file"'' loom/.githooks/pre-commit'
          exit_code: 0
          description: "Current hook contains the unconditional re-stage seam that overwrites a partial index"
      after_stage:
        - command: 'mkdir -p target/tmp && TMPDIR="$PWD/target/tmp" ./scripts/test-pre-commit-partial-staging.sh'
          exit_code: 0
          description: "Real hook rejects partial staging before mutation and preserves both byte streams"
      acceptance:
        - 'mkdir -p target/tmp && TMPDIR="$PWD/target/tmp" ./scripts/test-pre-commit-partial-staging.sh'
        - "./scripts/check-hook-syntax.sh"
        - 'git config --get core.hooksPath | rg -qx "loom/.githooks"'
      files:
        - "loom/.githooks/pre-commit"
        - "scripts/test-pre-commit-partial-staging.sh"
        - ".github/workflows/ci.yml"
      working_dir: "."
      artifacts:
        - "loom/.githooks/pre-commit"
        - "scripts/test-pre-commit-partial-staging.sh"
        - ".github/workflows/ci.yml"
      wiring:
        - source: ".github/workflows/ci.yml"
          pattern: "test-pre-commit-partial-staging\\.sh"
          description: "CI executes the black-box partial-staging regression"
      wiring_tests:
        - name: "partial staging remains byte-preserved"
          command: 'mkdir -p target/tmp && TMPDIR="$PWD/target/tmp" ./scripts/test-pre-commit-partial-staging.sh'
          success_criteria:
            exit_code: 0

    - id: integration-verify
      name: "Integration Verification"
      stage_type: integration-verify
      model: "opus"
      reasoning_effort: "high"
      description: |
        Verify the completed behavior and review the final diff inside the enabled
        sandbox. Use only the narrowly granted /tmp/loom-pre-commit-plan directory
        for non-repository fixtures, and unset RUSTC_WRAPPER on Cargo commands so
        the denied sccache Unix socket cannot interfere with verification.
        Use parallel subagents and skills to maximize performance.

        CONTEXT: read this plan, loom memory show --all, and the relevant knowledge
        sections on testing/lint and Git workflow. NEVER use editor auto-memory.

        CODE REVIEW: spawn parallel loom-code-reviewer subagents for shell/index
        correctness, test discrimination/coverage, and CI/sandbox integration.
        The orchestrator is Opus at high effort. Fix every finding before completion;
        do not weaken or delete an assertion to obtain green.

        FUNCTIONAL: run the checked-in fixture against the checked-in hook, confirm
        core.hooksPath selects loom/.githooks, and verify the CI consumer invokes
        the fixture. Record discoveries with loom memory for knowledge-distill.
      dependencies: ["harden-pre-commit"]
      setup:
        - "mkdir -p /tmp/loom-pre-commit-plan"
      acceptance:
        - 'mkdir -p target/tmp && TMPDIR="$PWD/target/tmp" ./scripts/test-pre-commit-partial-staging.sh'
        - "./scripts/check-hook-syntax.sh"
        - 'mkdir -p target/tmp && TMPDIR="$PWD/target/tmp" bash loom-hooks/tests/run-all.sh'
        - "cd loom && env -u RUSTC_WRAPPER cargo build --all-targets"
        - "cd loom && cargo fmt --check"
        - "cd loom && env -u RUSTC_WRAPPER cargo clippy --all-targets -- -D warnings"
        - 'cd loom && env -u RUSTC_WRAPPER RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps'
        - 'cd loom && cargo audit --no-fetch -d "$HOME/.cargo/advisory-db"'
        - "cd loom && TMPDIR=/tmp/loom-pre-commit-plan env -u RUSTC_WRAPPER -u LOOM_STAGE_ID -u LOOM_SESSION_ID -u LOOM_WORKTREE_PATH -u GIT_INDEX_FILE -u GIT_DIR -u GIT_WORK_TREE cargo test --all-targets --no-fail-fast"
        - 'git config --get core.hooksPath | rg -qx "loom/.githooks"'
      working_dir: "."
      artifacts:
        - "loom/.githooks/pre-commit"
        - "scripts/test-pre-commit-partial-staging.sh"
      wiring:
        - source: ".github/workflows/ci.yml"
          pattern: "test-pre-commit-partial-staging\\.sh"
          description: "The regression is selected by the CI hook job"
      wiring_tests:
        - name: "repository hook protects a partial index"
          command: 'mkdir -p target/tmp && TMPDIR="$PWD/target/tmp" ./scripts/test-pre-commit-partial-staging.sh'
          success_criteria:
            exit_code: 0

    - id: knowledge-distill
      name: "Knowledge Distillation"
      stage_type: knowledge-distill
      model: "opus"
      reasoning_effort: "high"
      description: |
        Curate plan memories and update the existing partial-staging incident with
        the shipped fail-before-mutation invariant and regression path.
        SINGLE-AGENT: do not spawn subagents. The orchestrator is Opus at high effort.
        NEVER use editor auto-memory.

        Read this plan, loom memory show --all, and current knowledge. Apply every
        stale-knowledge memory first with loom knowledge replace-section, then use
        loom knowledge update for new concise findings. Follow hierarchical tier
        routing: findings of roughly 40 lines or fewer stay in the tier-1 summary;
        larger material belongs in a category/slug topic with a short summary link.
        INDEX.md regenerates on each knowledge write; run loom review afterward.

        Give every memory a receipt immediately with loom memory resolve, using a
        target for promoted/merged entries and a reason for discarded/deferred
        entries. README and CONTRIBUTING require no change unless implementation
        introduces user-facing behavior beyond the repository hook; record that
        decision in memory. Finish by resolving every pending memory.
      dependencies: ["integration-verify"]
      acceptance:
        - 'rg -q "## " doc/loom/knowledge/mistakes.md'
        - "loom knowledge check --strict"
        - "loom memory pending --strict"
      files:
        - "doc/loom/knowledge/**"
        - "README.md"
        - "CONTRIBUTING.md"
      working_dir: "."
```

<!-- END loom METADATA -->
