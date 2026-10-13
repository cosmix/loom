# Stack & Dependencies

> Dependencies, frameworks, tooling
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [architecture.md](architecture.md) for how dependencies are used.

## Core Stack

- **Language:** Rust (~380K lines under `loom/src`)
- **Concurrency:** std threads only. The daemon is thread-based: a Unix socket accept loop, a client worker pool, and the orchestrator, log-tailer, status-broadcaster, and quota-poller threads, each joined with a 5 s timeout on shutdown (`loom/src/daemon/server/lifecycle.rs`). `loom/Cargo.toml` has no tokio and no async runtime.
- **CLI Framework:** clap with `#[derive(Parser)]`
- **Serialization:** serde; YAML through `loom::yaml` (serde-saphyr), JSON through serde_json, toml
- **Error Handling:** `anyhow` at application/orchestration boundaries with context chaining; typed
  domain errors where callers branch on outcomes

## Key Dependencies (Cargo.toml)

| Crate           | Purpose                                     |
| --------------- | ------------------------------------------- |
| clap            | CLI argument parsing                        |
| serde           | Serialization framework                     |
| serde-saphyr    | YAML via `loom::yaml` (plans, frontmatter)  |
| serde_json      | JSON, and dynamic YAML values               |
| anyhow          | Application errors and context              |
| reqwest         | Blocking HTTP (self-update, quota)          |
| toml            | Config file parsing                         |
| chrono          | Timestamps                                  |
| minisign-verify | Self-update signature verification          |
| ratatui         | Terminal UI for status dashboard            |
| serial_test     | Test isolation                              |
| tempfile        | Temporary directories for tests             |
| fs2             | File locking                                |

## Build Tools

- **Cargo:** Standard Rust build system
- **Preferred Package Managers:** `cargo add`, `bun add`, `uv add` (never hand-edit manifests)

## Testing Stack

- Unit tests: `#[test]` with tempfile for isolation
- Integration tests: `loom/tests/integration/` with `serial_test` crate
- Serial test isolation required for many tests (git operations, daemon)

## Skills Dependencies

- `crate::parser::frontmatter::extract_frontmatter_raw` + `crate::yaml::from_str`: SKILL.md frontmatter deserialized straight into `SkillMetadata` (`loom/src/skills/index.rs`)

## Map Dependencies

- std::fs, std::path: File system traversal for project detection
- crate::fs::knowledge::KnowledgeDir: Knowledge file management integration

## Removed Dependencies

- `clap_complete` — removed in favor of custom shell completion scripts that call `loom complete` for dynamic completions

## External Agent Binaries (`loom pressure`)

- `codex` CLI — required at runtime by `loom pressure` for the Codex review rounds. Resolved by `loom/src/codex.rs::find_codex_path` (`which::which`, then `~/.bun/bin`, `~/.local/bin`, `~/.npm-global/bin`, `~/.cargo/bin`, `/usr/local/bin`, `/opt/homebrew/bin`). Typically installed via bun/npm.
- `sccache` — optional; when installed, loom exports `RUSTC_WRAPPER` into every session and acceptance run so dependency compiles are shared across worktrees (`orchestrator/terminal/native/build_cache.rs`).
- `claude` CLI — likewise required by `loom pressure` (resolved by `find_claude_path`).

- `tmux` — **optional** runtime dependency, required only when the terminal backend is set to `tmux`
  (`[terminal] backend = "tmux"` in `.loom/work/config.toml`, or `loom run --backend tmux`). No new Rust
  crates were needed for the backend: it shells out to the `tmux` binary and reuses `which` (PATH
  probe), `libc` (`getuid()` for the `tmux-<uid>` socket dir) and `sha2` (the per-repo overview viewer
  socket name), all already in `loom/Cargo.toml`. Availability is probed with `which::which("tmux")` at
  `loom init` (advisory warning only), at `loom run` startup (refuses to start when the effective
  backend is tmux and tmux is not on PATH), and again per spawn (a configured-tmux spawn with no tmux
  on PATH, or a tmux spawn failure, returns `Err` and blocks the stage — see
  [Terminal Backends](architecture/terminal-backends.md) § Tmux Unavailable or Failing; no lane switch
  ever happens). Version note: the overview's nested-attach and layout behaviour was verified against
  tmux 3.7b.

## Hook Runtime Dependencies (jq, rg, fd)

`jq` is a hard requirement: every hook under `loom-hooks/` parses the Claude Code hook payload with it.
Checked by `install.sh check_runtime_tools` (exits 1 if absent), `loom/src/commands/run/checks.rs::require_jq`
(hard-fails `loom run`), and `loom/src/commands/repair/settings_checks.rs::jq_missing_issue` (reports it,
no auto-fixer). In the hooks themselves, blocking guards call `loom_require_jq` (`loom-hooks/_common.sh`,
exit 2, fail closed) and advisory hooks call `loom_warn_no_jq` (exit 1, non-blocking); lifecycle hooks
(`session-start.sh`, `post-tool-use.sh`, `subagent-start.sh`, `subagent-stop.sh`) keep their own
pre-existing `command -v jq` skip instead. `rg`/`fd` are doctrine dependencies only (CLAUDE.md rule 8):
`install.sh` and `loom run`'s `advisory_search_tools_preflight` warn when either is missing, and
`loom-hooks/prefer-modern-tools.sh` allows `grep`/`find` through with a warning rather than blocking when the
preferred replacement is not installed.

## Tree-sitter Source Extraction

Thirteen optional dependencies behind three default-on cargo features (`loom/Cargo.toml`), all exact-pinned
with `=`:

| Pack (feature) | Crates |
| --- | --- |
| Core (`source-graph`) | `tree-sitter =0.27.0`, `tree-sitter-rust =0.24.2`, `tree-sitter-typescript =0.23.2` (TypeScript and TSX), `tree-sitter-python =0.25.0`, `tree-sitter-go =0.25.0`, `tree-sitter-javascript =0.25.0`, `streaming-iterator =0.1.9` |
| Wave B (`source-graph-wave-b`) | `tree-sitter-java =0.23.5`, `tree-sitter-c-sharp =0.23.5`, `tree-sitter-ruby =0.23.1`, `tree-sitter-php =0.24.2` |
| Wave C (`source-graph-wave-c`) | `tree-sitter-c =0.24.2`, `tree-sitter-cpp =0.23.4` |

`default = ["source-graph", "source-graph-wave-b", "source-graph-wave-c"]`; each wave feature implies
`source-graph`. `streaming-iterator` is required, not incidental: `QueryCursor::matches` returns a
`StreamingIterator`. Every grammar comes from `github.com/tree-sitter`; Kotlin and Swift grammars are not
maintained there, so those dialects are not supported, and shell and configuration languages (proposal
wave D) are out of scope.

**The per-pack capability-gap rule.** The pack, not the crate, is the unit of feature gating. A pack that is not
compiled does not fail the build or hide its dialects: `extractor_for` returns `Lookup::Gap` and the files get
file-level `LexicalOnly` nodes with the detail `grammar pack {feature} not compiled ({dialect})`, which the coverage
report and census list as `gaps`. Collapsing every grammar into one feature would let a host drop half the grammars
and leave the registry inconsistent, and a feature per crate (what `cargo add --optional` generates) would expose
that same inconsistency; packs group grammars that ship together. `GrammarPack::feature()` names the feature and
`compiled()` tests it. `--no-default-features` remains the degraded mode that builds with no C toolchain and yields
lexical nodes only. The `[features]` table is the one hand-edited block of `Cargo.toml`: it has no `cargo` command;
dependencies themselves are added with `cargo add <crate>@=<version> --optional`.

The three packs cost binary size and build time: the release binary grew from 37.7 MiB (core grammars only) to 52.0 MiB
(+37.9%) with both waves, and the grammar build scripts that compile `parser.c` take 0.5 to 1.7 s each (cpp and c-sharp
1.7 s, ruby and php 1.4 s, javascript 0.8 s, java 0.6 s, c 0.5 s).

Exact pins matter because `ExtractorIdentity.grammar_version` carries the *grammar* crate version into the cache
identity: a floating pin would silently invalidate or, worse, silently reuse cached extractions. A core-only
`tree-sitter` bump does not change it. Upgrading the core crate is not free: 0.26 to 0.27 made
`QueryMatch::captures` a method rather than a public field (`context/extract/treesitter/collect.rs`). See
[Source Graph](architecture/source-graph.md#the-extractor-trait).
