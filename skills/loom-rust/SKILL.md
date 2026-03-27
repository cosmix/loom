---
name: loom-rust
description: "Rust language expertise for writing safe, performant, production-quality Rust code in the Loom project. Use when working with Rust development, ownership and borrowing patterns, error handling, async/await with tokio, cargo workspace management, CLI tools with clap, and serialization with serde. Rust is the primary language for Loom."
allowed-tools: Read, Grep, Glob, Edit, Write, Bash
trigger-keywords: rust, cargo, rustc, ownership, borrowing, lifetime, trait, impl, struct, enum, Result, Option, async, await, tokio, serde, clap, thiserror, anyhow, Arc, Mutex, RwLock, RefCell, Box, Rc, Vec, HashMap, derive, macro
---

# Rust Language Expertise

## Overview

This skill provides guidance for writing safe, efficient, and idiomatic Rust code. As the primary language for the Loom project, it covers ownership and borrowing, error handling, traits and generics, async programming with tokio, CLI development with clap, serialization with serde, testing strategies, and cargo workspace management.

## Ownership, Borrowing, and Lifetimes

The agent should follow these ownership rules when writing or reviewing Rust code:

1. Each value has exactly one owner.
2. When the owner goes out of scope, the value is dropped.
3. Ownership can be transferred (moved) or borrowed via references.

### Borrowing and References

- Immutable borrows (`&T`) allow read-only access; multiple simultaneous borrows are permitted.
- Mutable borrows (`&mut T`) allow read-write access; only one mutable borrow at a time.
- The agent should prefer borrowing over cloning to avoid unnecessary allocations.

### Lifetimes

- Lifetime annotations (`'a`) ensure references remain valid for their usage scope.
- The agent should rely on lifetime elision where the compiler infers lifetimes automatically.
- Explicit annotations are needed when a function returns a reference derived from multiple inputs.

```rust
// Lifetime annotation example
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Struct with lifetime
struct Parser<'a> {
    input: &'a str,
    position: usize,
}
```

## Error Handling

The agent should use `thiserror` for library error types and `anyhow` for application-level code.

### Library Errors with thiserror

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("Not found: {0}")]
    NotFound(String),
}
```

### Application Errors with anyhow

```rust
use anyhow::{Context, Result, bail, ensure};

fn read_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config from {}", path))?;

    let config: Config = serde_json::from_str(&content)
        .context("Failed to parse config JSON")?;

    ensure!(!config.name.is_empty(), "Config name cannot be empty");

    if config.port == 0 {
        bail!("Invalid port number");
    }

    Ok(config)
}
```

### Option and Result Patterns

- Use the `?` operator to propagate errors up the call stack.
- Convert `Option` to `Result` with `.ok_or_else()` for meaningful error messages.
- Use `.with_context()` from anyhow to add contextual information to errors.
- Collect results with `.collect::<Result<Vec<_>>>()` to fail on the first error.

## Traits and Generics

The agent should define traits to express shared behavior and use generics with trait bounds for type-safe abstractions.

```rust
// Trait definition with default method
trait Repository<T> {
    fn get(&self, id: &str) -> Option<&T>;
    fn save(&mut self, item: T) -> Result<(), Box<dyn std::error::Error>>;

    fn exists(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
}

// Where clauses for complex bounds
fn merge<T, U, V>(a: T, b: U) -> V
where
    T: IntoIterator<Item = V>,
    U: IntoIterator<Item = V>,
    V: Ord + Clone,
{
    // implementation
}
```

### Key Trait Patterns

- **Associated types** over generic parameters when a trait has one natural type per implementation.
- **Blanket implementations** (`impl<T: Display> ToString for T`) to extend behavior generically.
- **Trait objects** (`Box<dyn Trait>`) for dynamic dispatch when concrete types are unknown at compile time.

## Iterators

The agent should prefer iterator chains over manual loops for clarity and performance.

```rust
fn process_users(users: Vec<User>) -> Vec<String> {
    users
        .into_iter()
        .filter(|u| u.active)
        .map(|u| u.email)
        .filter_map(|email| email)
        .collect()
}
```

Key methods: `filter`, `map`, `filter_map`, `fold`, `any`, `all`, `find`, `partition`, `enumerate`, `zip`, `collect`.

## Async/Await with Tokio

The agent should use tokio as the async runtime for all async operations in Loom.

### Core Patterns

```rust
use tokio::sync::{mpsc, Mutex, RwLock};
use std::sync::Arc;
use tokio::time::{sleep, Duration, timeout};

// Basic async function
async fn fetch_data(url: &str) -> Result<String> {
    let response = reqwest::get(url).await?;
    Ok(response.text().await?)
}

// Timeout for async operations
async fn fetch_with_timeout(url: &str) -> Result<String> {
    timeout(Duration::from_secs(5), fetch_data(url))
        .await
        .context("Request timed out")?
}

// Shared state with Arc<Mutex<T>>
struct AppState {
    counter: Arc<Mutex<u64>>,
    cache: Arc<RwLock<HashMap<String, String>>>,
}
```

### Channel Communication

- **mpsc** for multi-producer, single-consumer messaging between tasks.
- **broadcast** for multi-consumer messaging where all receivers get every message.
- **oneshot** for single-value responses (request-reply patterns).
- Use `tokio::task::spawn_blocking` for CPU-intensive work that would block the async runtime.
- Use `tokio::select!` to race multiple futures and proceed with the first to complete.

## Cargo and Project Structure

### Cargo.toml Essentials

```toml
[package]
name = "myproject"
version = "0.1.0"
edition = "2021"

[dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
anyhow = "1.0"

[dev-dependencies]
criterion = "0.5"
mockall = "0.12"
```

### Workspace Structure

```toml
# Root Cargo.toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.35", features = ["full"] }
```

The agent should use workspace dependencies to keep versions consistent across crates.

## CLI Development with Clap

The agent should use clap's derive API for CLI argument parsing.

```rust
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "loom", about = "Agent orchestration CLI", version)]
struct Cli {
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init { name: String },
    Run {
        #[arg(value_name = "PLAN")]
        plan: Option<PathBuf>,
    },
    Status {
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OutputFormat { Table, Json, Yaml }
```

## Serialization with Serde

### Common Derive Patterns

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    name: String,
    #[serde(default)]
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(rename = "userId")]
    user_id: String,
}

// Tagged enum for polymorphic serialization
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Message {
    Text { content: String },
    Image { url: String, alt: Option<String> },
}
```

### Key Serde Attributes

- `#[serde(default)]` uses `Default::default()` for missing fields.
- `#[serde(skip_serializing_if = "Option::is_none")]` omits `None` values from output.
- `#[serde(rename = "...")]` maps Rust field names to different serialized names.
- `#[serde(flatten)]` inlines nested struct fields into the parent.
- `#[serde(tag = "type")]` for internally tagged enums (common for JSON APIs).
- `#[serde(untagged)]` tries each variant in order (useful for `StringOrNumber` unions).

## Common Patterns

### Builder Pattern

The agent should use the builder pattern for structs with many optional fields.

```rust
#[derive(Default)]
pub struct RequestBuilder {
    url: Option<String>,
    headers: HashMap<String, String>,
    timeout: Duration,
}

impl RequestBuilder {
    pub fn new() -> Self { Self { timeout: Duration::from_secs(30), ..Default::default() } }
    pub fn url(mut self, url: impl Into<String>) -> Self { self.url = Some(url.into()); self }
    pub fn header(mut self, key: impl Into<String>, val: impl Into<String>) -> Self {
        self.headers.insert(key.into(), val.into()); self
    }
    pub fn build(self) -> Result<Request, BuildError> {
        let url = self.url.ok_or(BuildError::MissingUrl)?;
        Ok(Request { url, headers: self.headers, timeout: self.timeout })
    }
}
```

### Newtype Pattern

The agent should use newtypes for type-safe domain identifiers to prevent mixing up IDs at compile time.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(String);

impl UserId {
    pub fn new(id: impl Into<String>) -> Self { UserId(id.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}
```

### Smart Pointers Quick Reference

| Type | Use Case |
|------|----------|
| `Box<T>` | Heap allocation, single owner, recursive types |
| `Rc<T>` | Shared ownership, single-threaded |
| `Arc<T>` | Shared ownership, multi-threaded |
| `RefCell<T>` | Interior mutability, single-threaded (runtime borrow checks) |
| `Mutex<T>` | Interior mutability, multi-threaded (blocking lock) |
| `RwLock<T>` | Interior mutability, multi-threaded (read-heavy workloads) |
| `Weak<T>` | Non-owning reference to break reference cycles |

Common combinations: `Arc<Mutex<T>>` for shared mutable state across threads; `Rc<RefCell<T>>` for shared mutable state in single-threaded contexts.

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        assert_eq!(add(2, 3), 5);
    }

    #[test]
    fn test_with_result() -> Result<(), Box<dyn std::error::Error>> {
        let config = parse_config("valid config")?;
        assert_eq!(config.name, "test");
        Ok(())
    }

    #[test]
    #[should_panic(expected = "divide by zero")]
    fn test_panic() { divide(1, 0); }

    #[tokio::test]
    async fn test_async() {
        let result = fetch_data("http://example.com").await;
        assert!(result.is_ok());
    }
}
```

## Anti-Patterns to Avoid

| Anti-Pattern | Preferred Approach |
|---|---|
| Unnecessary `.clone()` in loops | Iterate by reference (`&[T]` instead of `&Vec<T>`) |
| `.unwrap()` / `.expect()` in library code | Return `Result` and let the caller handle errors |
| String concatenation in loops (`result + part`) | Use `parts.join(", ")` or `push_str` |
| `Box<dyn Error>` in library APIs | Use concrete error types via `thiserror` |
| `unsafe` without documented invariants | Safe by default; `unsafe` only with `// SAFETY:` comment and `debug_assert!` |
| Ignoring `#[must_use]` results | Handle with `?` or explicitly ignore with `.ok()` |
| Excessive `Rc<RefCell<T>>` for graphs | Consider arena allocation with index-based references |
