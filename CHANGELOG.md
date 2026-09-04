# Changelog

All notable changes to loom are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-09-04

First published release.

### Added

- **Plan-driven orchestration** — `loom init` parses a markdown plan with embedded YAML from `doc/plans/` into a stage dependency DAG; `loom run` starts the daemon and orchestrator, `loom status` tracks progress, and `loom stop` shuts it down.
- **Parallel execution and progressive merge** — independent stages run concurrently, each in its own git worktree (`.worktrees/<stage-id>`, branch `loom/<stage-id>`); completed stages merge back progressively under a file lock, and a real conflict spawns a dedicated resolution session instead of stalling the run.
- **Enforced verification** — `loom stage complete` runs the stage's acceptance criteria itself before completing, then goal-backward checks (`artifacts`, `wiring`, `wiring_tests`, `dead_code_check`, `before_stage`, `after_stage`) confirm the outcome exists rather than trusting the agent's report; the `--no-verify`, `--force-unsafe`, and `--assume-merged` bypass flags require an operator-only proof.
- **Deterministic guardrails** — 16 Claude Code hooks plus a git `pre-commit` hook enforce commit discipline, staging scope, worktree boundaries, and subagent limits outside the model's control.
- **Knowledge capture, distillation and retrieval** — agents journal to a per-stage memory with `loom memory`; a `knowledge-distill` stage curates memories into a tiered knowledge base under `doc/loom/knowledge/`; retrieval is offline and deterministic via `loom knowledge context`, and every stage gets a per-stage Knowledge Brief built through the same path, backed by a source graph of code symbols.
- **Model allocation by delegation** — every stage's main agent orchestrates on Opus; implementation is delegated by agent type to Sonnet, Opus, Fable, or the codex lane, so cost savings come from delegation rather than downgrading the orchestrator.
- **Crash recovery and liveness** — all orchestration state lives in plain files under `.work/`; the daemon polls every 5s, tracks PID liveness and per-session heartbeats, classifies failures into retryable and needs-diagnosis, and recovers orphaned sessions on restart.
- **Terminal backends** — the `native` backend opens a terminal-emulator window per session; the `tmux` backend runs sessions in detached tmux servers for headless or SSH use, with `loom attach` for a tiled overview or a single stage.
- **Sandboxing and command confinement** — plan-level defaults and per-stage overrides control filesystem reads/writes, network domains, and permission mode for the agent session; `confined` command confinement rebuilds the environment for plan-authored commands so they cannot read ambient credentials.
- **Plan hardening** — `loom plan verify` validates a plan with no side effects before you spend anything, and `loom pressure` hardens it through adversarial review rounds run by two different model families.
- **Human-in-the-loop stage states** — `WaitingForInput`, `NeedsHumanReview`, `Blocked`, `MergeConflict`, and `NeedsAdjudication` make "needs a person" an explicit outcome, with `loom stage hold/release/skip/retry/human-review/dispute-criteria` to act on them.
- **Live ledger dashboard** — `loom status --live` renders a per-stage table across state, dependencies, models, activity, context usage, time, and merge status, with a legend overlay.
- **Signed self-update** — `loom update` verifies downloaded binaries against a minisign signature and the public key embedded in the binary.
- **Cost reporting** — `loom usage` reports what agent sessions actually consumed, in tokens, filterable by stage, plan, or project.
- **Shell completions** — `loom completions --install` writes context-aware tab completions for bash, zsh, and fish.

### Platforms

- Signed binaries for Linux x86_64, macOS Apple Silicon and macOS Intel, published on the GitHub release with `.minisig` signatures and `SHA256SUMS.txt`.
- Windows is supported through WSL2: the Linux x86_64 binary runs unmodified, with the tmux backend covering the GUI terminal a stock WSL install lacks.
- Linux ARM64 builds from source; no binary is published yet.

[0.5.0]: https://github.com/cosmix/loom/releases/tag/v0.5.0
