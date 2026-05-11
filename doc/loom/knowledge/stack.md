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

## Skills Dependencies

- serde + serde_yaml: YAML frontmatter parsing for SKILL.md files
- crate::parser::frontmatter::extract_yaml_frontmatter: Shared YAML extraction utility

## Map Dependencies

- std::fs, std::path: File system traversal for project detection
- crate::fs::knowledge::KnowledgeDir: Knowledge file management integration

## Container Backend Dependencies

| Crate | Purpose |
| --- | --- |
| sha2 | SHA-256 for image fingerprint computation |
| hex | Hex encoding for fingerprint output |
| handlebars | Dockerfile.tmpl rendering (language flags) |
| shell-escape | Safe shell argument escaping for container spawn args |

## Container Runtime Support

Supported runtimes (auto-detected at spawn time via `container/runtime.rs`):

- **Docker** — `docker` binary
- **Podman** — `podman` binary
- **Apple Container** — `/usr/local/bin/container` with Apple-signature `container --version` output (macOS only)

Detection order: Docker → Podman → Apple Container. Apple Container detection requires both binary existence AND version output validation to avoid collision with unrelated tools named `container`.

## Embedded Resources (`loom/resources/`)

| File | Purpose | Embedded via |
| --- | --- | --- |
| `Dockerfile.tmpl` | Container image template (handlebars) | `include_str!()` in resources.rs |
| `firewall.sh` | Egress firewall (image-resident, capability drop) | `include_str!()` in resources.rs |
| `entrypoint.sh` | Container entrypoint | `include_str!()` in resources.rs |
| `claude-settings.json` | Claude Code settings template for container | `include_str!()` in resources.rs |

## Removed Dependencies

- `clap_complete` — removed in favor of custom shell completion scripts that call `loom complete` for dynamic completions
