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
- **Error Handling:** anyhow with context chaining

## Key Dependencies (Cargo.toml)

| Crate       | Purpose                            |
| ----------- | ---------------------------------- |
| clap        | CLI argument parsing               |
| serde       | Serialization framework            |
| serde_yaml  | YAML parsing for frontmatter       |
| anyhow      | Error handling with context        |
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
