# Completion Runtime

> Cargo churn, stall kills

## A Per-Command Cargo Home Rebuilt Every Dependency, and Stall Recovery Killed the Completion (2026-10-20)

**What happened:** stage `doctrine-and-guards` (PLAN-01-doctrine-truth-and-plan-guards) committed its work, passed its own gate and review, then never completed.

- Three sessions in a row ran the pinned `loom stage complete`. Each printed only "Running acceptance criteria…" and was killed by stall recovery after about 2700 s of silence (exit 137). The third was parked for human review.
- Every confined acceptance command had a fresh random `CARGO_HOME` (`CargoScratch`, 56f4d32d, moved outside the tree by b78c93e9). Cargo marks every registry package "Dirty: the path to the source changed" when `CARGO_HOME` changes.
- Measured on loom: `cargo test --lib --no-run` took 0.19 s warm and about 96 s with a fresh home (260 units). With about 31 compiling cargo criteria, the acceptance list ran past 45 minutes.
- The session's own verifier ran the same criteria from the session shell with `~/.cargo` in about 15 minutes, so the gate looked green.

**Why:** two independent defects compounded.

1. A path Cargo fingerprints changed on every command.
2. Heartbeats come only from tool hooks, so one long foreground `loom stage complete` reads as silence, and stall recovery escalated at three response budgets whatever the session was running.

Three things hid the cause:

- In a session the acceptance cache cannot store passes (the state directory is read-only), so every retry restarted from zero.
- Results print only after the last criterion, so the killed output never named the slow step.
- The session's Bash output cap (`BASH_MAX_OUTPUT_LENGTH=12000`) can also cut the per-criterion results of a run that does finish.

**Prevention:**

- Measure an acceptance list through the confined runner (`loom check <stage> --no-cache`, or the real completion), never only from the session shell, which does not exercise the runner's environment.
- Every directory a build tool fingerprints by path must stay stable across commands.
- A long command the daemon can see coming gets an explicit liveness signal, not a larger global budget.

**Fix:**

- Stable `CARGO_HOME`: see [execution containment](../architecture/execution-containment.md#read-this-first-what-containment-means-in-loom). On Linux it is the host path with private writable state, and a host-warm target dir stays warm (261 units fresh in 0.4 s).
- Stall line: see [orchestrator loop](../architecture/orchestrator-loop.md#stall-escalation-during-a-running-completion).

## A Stable Per-Tree Cargo Home Is Writable by the Stage Agent (2026-10-20)

**What happened:** review of the stable-home fix found that the per-tree `.loom-cargo-<digest>` home (macOS, and a rare Linux fallback) is a predictable name in a temporary root the stage session can write.

- An agent could pre-create it, or edit it between commands, with a Cargo config file setting a target `runner = "true"`, `rustflags`, `[env]` or a `[source]` replacement.
- Every `cargo test` criterion would then "pass" without running.
- Nothing in the worktree diff, the review or the acceptance cache fingerprint would show it.
- The random per-command home it replaced could not be pre-seeded.

**Why:** in a session, the confined runner runs as the same user and inside the same sandbox as the agent, so anything it can create, the agent can create or list. An unguessable name does not help once the directory exists.

**Prevention:** persistent state that the in-session runner reads is reset to an allowlist on every use, through descriptor-relative operations that never follow links, and a test seeds the hostile entries.

**Fix:** `verify/criteria/confine_linked_home.rs::populate`. Writes made after populate survive until the next populate; closing that window needs a home the agent cannot write.
