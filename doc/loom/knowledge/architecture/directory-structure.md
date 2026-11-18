# Directory Structure

> Tree

## Directory Structure

```text
loom/src/
  main.rs, lib.rs          # CLI entry (clap), module exports
  yaml.rs                   # YAML from_str/to_string over serde-saphyr (strict booleans)
  commands/                 # CLI implementations
    init/, run/, stage/ (complete, merge, merge_resolver, merge_verify, ...),
    status/, merge/, memory/, knowledge/, track/, runner/
  daemon/server/            # Background daemon
    core.rs, lifecycle.rs, protocol.rs, status.rs, client.rs, orchestrator.rs
  orchestrator/             # Core engine
    core/                   # Main loop, stage executor, persistence, recovery
    terminal/               # SessionBackend dispatcher
      native/               # Host OS terminal spawning (11+ emulators)
      tmux/                 # Detached, per-session tmux servers
    monitor/                # Session health, heartbeat, failure tracking
    liveness.rs             # LivenessService — session liveness probe
    signals/                # Signal generation (Manus format, cache, CRUD)
    continuation/           # Context handoff management
    progressive_merge/      # Merge orchestration + lock
    auto_merge.rs
    merge_attribution.rs    # Attribute global MERGE_HEAD to a stage; reconcile
  models/                   # Domain models
    stage/ (types, transitions, methods)
    session/ (types, methods)
  plan/                     # Plan parsing
    parser.rs, schema/ (types, validation), graph/ (DAG builder)
  fs/                       # File operations
    work_dir.rs, knowledge.rs, memory.rs
  git/                      # Git operations
    worktree/ (base, operations), merge/ (mod, in_progress, lock, status), branch/
  verify/                   # Acceptance + goal-backward verification
    criteria/, transitions/, goal_backward/
  sandbox/                  # Claude Code sandbox config generation
    config.rs, settings.rs
  hooks/                    # Hook script definitions
  parser/frontmatter.rs     # Canonical YAML frontmatter extraction
  validation.rs             # Input validation (IDs, names)
  completions/              # Shell completion (custom scripts + dynamic engine + install)
  process/                  # Bounded subprocesses, minimal environments, PID identity

.loom/work/                      # Runtime state (gitignored)
  config.toml, stages/*.md, sessions/*.md, signals/*.md,
  handoffs/*.md, orchestrator.sock, orchestrator.pid
```
