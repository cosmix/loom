# Stack & Dependencies

> Project technology stack, frameworks, and key dependencies.
> This file is append-only - agents add discoveries, never delete.
>
> **Related files:** [architecture.md](architecture.md) for how dependencies are used.

## Core Stack

- **Language:** Rust (~15K lines)
- **Async Runtime:** tokio (daemon, socket handling)
- **CLI Framework:** clap with `#[derive(Parser)]`
- **Serialization:** serde, serde_yaml, toml
- **Error Handling:** `anyhow` at application/orchestration boundaries with context chaining; typed
  domain errors where callers branch on outcomes

## Key Dependencies (Cargo.toml)

| Crate       | Purpose                            |
| ----------- | ---------------------------------- |
| clap        | CLI argument parsing               |
| serde       | Serialization framework            |
| serde_yaml  | YAML parsing for frontmatter       |
| anyhow      | Application errors and context     |
| tokio       | Async runtime (daemon)             |
| toml        | Config file parsing                |
| chrono      | Timestamps                         |
| minisign    | Self-update signature verification |
| ratatui     | Terminal UI for status dashboard   |
| serial_test | Test isolation                     |
| tempfile    | Temporary directories for tests    |
| fs2         | File locking                       |

## Build Tools

- **Cargo:** Standard Rust build system
- **Preferred Package Managers:** `cargo add`, `bun add`, `uv add` (never hand-edit manifests)

## Testing Stack

- Unit tests: `#[test]` with tempfile for isolation
- Integration tests: `loom/tests/integration/` with `serial_test` crate
- Serial test isolation required for many tests (git operations, daemon)

## Skills Dependencies

- serde + serde_yaml: YAML frontmatter parsing for SKILL.md files
- crate::parser::frontmatter::extract_yaml_frontmatter: Shared YAML extraction utility

## Map Dependencies

- std::fs, std::path: File system traversal for project detection
- crate::fs::knowledge::KnowledgeDir: Knowledge file management integration

## Removed Dependencies

- `clap_complete` — removed in favor of custom shell completion scripts that call `loom complete` for dynamic completions

## External Agent Binaries (`loom pressure`)

- `codex` CLI — required at runtime by `loom pressure` for the Codex review rounds. Resolved by `loom/src/codex.rs::find_codex_path` (`which::which`, then `~/.bun/bin`, `~/.local/bin`, `~/.npm-global/bin`, `~/.cargo/bin`, `/usr/local/bin`, `/opt/homebrew/bin`). Typically installed via bun/npm.
- `claude` CLI — likewise required by `loom pressure` (resolved by `find_claude_path`).

- `tmux` — **optional** runtime dependency, required only when the terminal backend is set to `tmux`
  (`[terminal] backend = "tmux"` in `.work/config.toml`, or `loom run --backend tmux`). No new Rust
  crates were needed for the backend: it shells out to the `tmux` binary and reuses `which` (PATH
  probe), `libc` (`getuid()` for the `tmux-<uid>` socket dir) and `sha2` (the per-repo overview viewer
  socket name), all already in `loom/Cargo.toml`. Availability is probed with `which::which("tmux")` at
  `loom init`, at `loom run` startup and again per spawn; a missing binary is always **advisory** —
  loom warns and falls back to the native lane, never aborts. Version note: the overview's nested-attach
  and layout behaviour was verified against tmux 3.7b.

## Tree-sitter Source Extraction (2026-08-17)

Six optional dependencies behind ONE default-on cargo feature, `source-graph`
(`loom/Cargo.toml:41-46`, `:63-77`), all exact-pinned with `=`:

| Crate | Pin |
| --- | --- |
| `tree-sitter` | `=0.26.12` |
| `tree-sitter-rust` | `=0.24.2` |
| `tree-sitter-typescript` | `=0.23.2` |
| `tree-sitter-python` | `=0.25.0` |
| `tree-sitter-go` | `=0.25.0` |
| `streaming-iterator` | `=0.1.9` |

`streaming-iterator` is required, not incidental: tree-sitter 0.26's
`QueryCursor::matches` returns a `StreamingIterator`, not a plain `Iterator`.

`--no-default-features` is the only supported degraded mode and it **builds** — extraction
falls back to file-level lexical nodes rather than failing — so a host without a C
toolchain can still build loom. The deps are collapsed into one feature deliberately, so a
host cannot disable half the grammars and leave the extractor registry inconsistent.

Exact pins matter here because a grammar version participates in the extraction cache
identity (`ExtractorIdentity.grammar_version`); a floating pin would silently invalidate or,
worse, silently reuse cached extractions. See `architecture/source-graph.md`.
