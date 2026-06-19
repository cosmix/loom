---
name: loom-rust
description: Rust language expertise for idiomatic, production-quality code. Use for ownership and lifetimes, error handling with anyhow/thiserror, async/await with tokio, cargo workspace management, CLI tools with clap, and serialization with serde. Primary language of the Loom project.
triggers:
  - rust
  - cargo
  - rustc
  - ownership
  - borrowing
  - lifetime
  - trait
  - impl
  - struct
  - enum
  - Result
  - Option
  - async
  - await
  - tokio
  - serde
  - clap
  - thiserror
  - anyhow
  - Arc
  - Mutex
  - RwLock
  - RefCell
  - Box
  - Rc
  - Vec
  - HashMap
  - HashSet
  - String
  - derive
  - macro
---

# Rust Language Expertise

## Overview

This skill provides guidance for writing safe, efficient, and idiomatic Rust code. As the primary language for the Loom project, this skill covers:

- Ownership, borrowing, and lifetimes
- Error handling with Result, Option, thiserror, and anyhow
- Traits, generics, and type system patterns
- Async programming with tokio runtime
- CLI development with clap
- Serialization with serde (JSON, TOML, YAML)
- Common patterns and anti-patterns
- Testing strategies
- Cargo and workspace management

## Key Concepts

### Ownership, Borrowing, and Lifetimes

```rust
// Ownership rules:
// 1. Each value has exactly one owner
// 2. When the owner goes out of scope, the value is dropped
// 3. Ownership can be transferred (moved) or borrowed

// Move semantics
fn take_ownership(s: String) {
    println!("{}", s);
} // s is dropped here

fn main() {
    let s = String::from("hello");
    take_ownership(s);
    // s is no longer valid here
}

// Borrowing (references)
fn borrow(s: &String) {
    println!("{}", s);
}

fn borrow_mut(s: &mut String) {
    s.push_str(" world");
}

// Lifetimes ensure references are valid
fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}

// Struct with lifetime annotations
struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Parser { input, position: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }
}

// Common lifetime elision patterns
impl Config {
    // fn get(&self, key: &str) -> Option<&str>
    // is short for:
    // fn get<'a, 'b>(&'a self, key: &'b str) -> Option<&'a str>
    fn get(&self, key: &str) -> Option<&str> {
        self.map.get(key).map(|s| s.as_str())
    }
}
```

### Error Handling

```rust
use std::error::Error;
use std::fmt;
use std::io;

// Using thiserror for custom errors
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Parse error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Validation failed: {0}")]
    Validation(String),
}

// Using anyhow for application code
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

// The ? operator for propagating errors
fn process_file(path: &str) -> Result<Vec<Record>, AppError> {
    let content = std::fs::read_to_string(path)?; // io::Error -> AppError via From
    let records = parse_records(&content)?;
    Ok(records)
}

// Option handling
fn find_user(users: &[User], name: &str) -> Option<&User> {
    users.iter().find(|u| u.name == name)
}

fn get_user_email(users: &[User], name: &str) -> Option<String> {
    users
        .iter()
        .find(|u| u.name == name)
        .and_then(|u| u.email.clone())
}

// Converting between Option and Result
fn require_user(users: &[User], name: &str) -> Result<&User, AppError> {
    users
        .iter()
        .find(|u| u.name == name)
        .ok_or_else(|| AppError::NotFound(format!("User: {}", name)))
}
```

### Traits and Generics

```rust
// Defining traits
trait Repository<T> {
    fn get(&self, id: &str) -> Option<&T>;
    fn save(&mut self, item: T) -> Result<(), Box<dyn Error>>;

    // Default implementation
    fn exists(&self, id: &str) -> bool {
        self.get(id).is_some()
    }
}

// Trait bounds
fn process<T: Clone + Debug>(item: &T) {
    let cloned = item.clone();
    println!("{:?}", cloned);
}

// where clauses for complex bounds
fn merge<T, U, V>(a: T, b: U) -> V
where
    T: IntoIterator<Item = V>,
    U: IntoIterator<Item = V>,
    V: Ord + Clone,
{
    let mut result: Vec<V> = a.into_iter().chain(b.into_iter()).collect();
    result.sort();
    result.dedup();
    result.into_iter().next().unwrap()
}

// Associated types
trait Iterator {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}

// Implementing traits
struct InMemoryRepo<T> {
    items: HashMap<String, T>,
}

impl<T: Clone> Repository<T> for InMemoryRepo<T> {
    fn get(&self, id: &str) -> Option<&T> {
        self.items.get(id)
    }

    fn save(&mut self, item: T) -> Result<(), Box<dyn Error>> {
        // Implementation
        Ok(())
    }
}

// Blanket implementations are legal only when the trait (or the type) is LOCAL to your crate
trait Summarize {
    fn summary(&self) -> String;
}
impl<T: Display> Summarize for T {
    fn summary(&self) -> String {
        format!("Display: {}", self)
    }
}

// NEVER implement ToString. std already provides: impl<T: Display + ?Sized> ToString for T
// (the impl below conflicts with std's + violates the orphan rule -> E0119, will not compile).
// Implement Display and to_string() comes for free:
struct MyType(u32);
impl Display for MyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
// my_type.to_string() now works
```

### Iterators

```rust
// Iterator combinators
fn process_users(users: Vec<User>) -> Vec<String> {
    users
        .into_iter()
        .filter(|u| u.active)
        .map(|u| u.email)
        .flatten()  // drop the None values (clippy::filter_map_identity rejects .filter_map(|x| x))
        .collect()
}

// Custom iterator
struct Counter {
    current: usize,
    max: usize,
}

impl Iterator for Counter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current < self.max {
            let val = self.current;
            self.current += 1;
            Some(val)
        } else {
            None
        }
    }
}

// Useful iterator methods
fn examples(numbers: Vec<i32>) {
    // Fold/reduce
    let sum: i32 = numbers.iter().fold(0, |acc, x| acc + x);

    // Any/all
    let has_positive = numbers.iter().any(|&x| x > 0);
    let all_positive = numbers.iter().all(|&x| x > 0);

    // Find
    let first_even = numbers.iter().find(|&&x| x % 2 == 0);

    // Partition
    let (evens, odds): (Vec<_>, Vec<_>) = numbers.iter().partition(|&&x| x % 2 == 0);

    // Enumerate
    for (index, value) in numbers.iter().enumerate() {
        println!("{}: {}", index, value);
    }

    // Zip
    let other = vec![1, 2, 3];
    let pairs: Vec<_> = numbers.iter().zip(other.iter()).collect();
}
```

## Best Practices

### Cargo and Project Structure

```toml
# Cargo.toml
[package]
name = "myproject"
version = "0.1.0"
# 2021 remains valid for existing code, but 2024 is the current edition
# (stable since Rust 1.85.0, 2025-02-20) — use it for new projects.
edition = "2024"
rust-version = "1.85"

[dependencies]
tokio = { version = "1.35", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
thiserror = "1.0"
anyhow = "1.0"

[dev-dependencies]
criterion = "0.5"
mockall = "0.12"

[features]
default = []
full = ["feature-a", "feature-b"]
feature-a = []
feature-b = ["dep:optional-dep"]

[[bench]]
name = "my_benchmark"
harness = false
```

### Workspace Structure

```text
myworkspace/
├── Cargo.toml          # Workspace root
├── crates/
│   ├── core/
│   │   ├── Cargo.toml
│   │   └── src/
│   ├── api/
│   │   ├── Cargo.toml
│   │   └── src/
│   └── cli/
│       ├── Cargo.toml
│       └── src/
```

```toml
# Root Cargo.toml
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.35", features = ["full"] }

# Centralize lint policy once (stable since Rust 1.74); member crates opt in below.
[workspace.lints.rust]
unsafe_code = "forbid"
[workspace.lints.clippy]
pedantic = "warn"
unwrap_used = "deny"
```

```toml
# Each member crate's Cargo.toml inherits the workspace lint + package config
[lints]
workspace = true
```

### CLI Applications with Clap

```rust
use clap::{Parser, Subcommand, ValueEnum, Args};
use std::path::PathBuf;

// Main CLI structure
#[derive(Parser)]
#[command(name = "loom")]
#[command(about = "Agent orchestration CLI", long_about = None)]
#[command(version)]
struct Cli {
    /// Optional config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Verbosity level (can be used multiple times: -v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new project
    Init {
        /// Project name
        name: String,

        /// Project template
        #[arg(short, long, default_value = "default")]
        template: String,

        /// Skip git initialization
        #[arg(long)]
        no_git: bool,
    },

    /// Run the orchestrator daemon
    Run {
        /// Plan file to execute
        #[arg(value_name = "PLAN")]
        plan: Option<PathBuf>,

        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },

    /// Stage management commands
    Stage(StageArgs),

    /// Knowledge base commands
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommands,
    },

    /// Show status
    Status {
        /// Output format
        #[arg(short, long, value_enum, default_value = "table")]
        format: OutputFormat,

        /// Watch mode - refresh every N seconds
        #[arg(short, long, value_name = "SECONDS")]
        watch: Option<u64>,
    },
}

#[derive(Args)]
struct StageArgs {
    #[command(subcommand)]
    command: StageCommands,
}

#[derive(Subcommand)]
enum StageCommands {
    /// Mark stage as complete
    Complete {
        /// Stage ID
        stage_id: String,
    },
    /// List all stages
    List {
        /// Show only active stages
        #[arg(short, long)]
        active: bool,
    },
    /// Show stage details
    Show {
        /// Stage ID
        stage_id: String,
    },
}

#[derive(Subcommand)]
enum KnowledgeCommands {
    /// Initialize knowledge base
    Init,
    /// List knowledge files
    List,
    /// Show knowledge content
    Show {
        /// Specific file to show (entry-points, patterns, conventions)
        file: Option<String>,
    },
    /// Update knowledge file
    Update {
        /// File to update
        file: String,
        /// Content to append
        content: String,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum OutputFormat {
    /// Human-readable table
    Table,
    /// JSON output
    Json,
    /// YAML output
    Yaml,
}

// Main function
fn main() -> Result<()> {
    let cli = Cli::parse();

    // Configure logging based on verbosity
    let log_level = match cli.verbose {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };
    env_logger::Builder::new().filter_level(log_level).init();

    // Load config if provided
    let config = if let Some(config_path) = cli.config {
        Config::load(&config_path)?
    } else {
        Config::default()
    };

    // Dispatch to command handlers
    match cli.command {
        Commands::Init { name, template, no_git } => {
            commands::init(&name, &template, !no_git)?;
        }
        Commands::Run { plan, foreground } => {
            commands::run(plan.as_deref(), foreground, &config)?;
        }
        Commands::Stage(args) => match args.command {
            StageCommands::Complete { stage_id } => {
                commands::stage::complete(&stage_id)?;
            }
            StageCommands::List { active } => {
                commands::stage::list(active)?;
            }
            StageCommands::Show { stage_id } => {
                commands::stage::show(&stage_id)?;
            }
        },
        Commands::Knowledge { command } => match command {
            KnowledgeCommands::Init => commands::knowledge::init()?,
            KnowledgeCommands::List => commands::knowledge::list()?,
            KnowledgeCommands::Show { file } => {
                commands::knowledge::show(file.as_deref())?
            }
            KnowledgeCommands::Update { file, content } => {
                commands::knowledge::update(&file, &content)?
            }
        },
        Commands::Status { format, watch } => {
            if let Some(interval) = watch {
                commands::status::watch(format, interval)?;
            } else {
                commands::status::show(format)?;
            }
        }
    }

    Ok(())
}

// Custom argument validators
fn validate_stage_id(s: &str) -> Result<String, String> {
    if s.is_empty() {
        return Err("Stage ID cannot be empty".to_string());
    }
    if !s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err("Stage ID must contain only alphanumeric characters, hyphens, and underscores".to_string());
    }
    Ok(s.to_string())
}
```

### Serialization with Serde

```rust
use serde::{Deserialize, Serialize, Deserializer, Serializer};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::PathBuf;

// Basic derive macros
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    name: String,
    version: String,
    #[serde(default)]
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

// Renaming fields
#[derive(Debug, Serialize, Deserialize)]
struct User {
    #[serde(rename = "userId")]
    user_id: String,
    #[serde(rename = "userName")]
    user_name: String,
    // Flatten nested structure
    #[serde(flatten)]
    metadata: HashMap<String, String>,
}

// Default values and skip
#[derive(Debug, Serialize, Deserialize)]
struct Settings {
    #[serde(default = "default_timeout")]
    timeout: u64,
    #[serde(default)]
    retries: u32,
    #[serde(skip)]
    runtime_state: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
}

fn default_timeout() -> u64 {
    30
}

// Enum representations
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Active,
    Inactive,
    Pending,
}

// Tagged enum (externally tagged by default)
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum Message {
    Text { content: String },
    Image { url: String, alt: Option<String> },
    Video { url: String, duration: u32 },
}

// Internally tagged enum
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data")]
enum Event {
    Created(String),
    Updated { id: String, changes: Vec<String> },
    Deleted(String),
}

// Untagged enum (tries each variant in order)
#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum StringOrNumber {
    Str(String),
    Num(i64),
}

// Custom serialization
#[derive(Debug)]
struct Timestamp(chrono::DateTime<chrono::Utc>);

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.0.timestamp())
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let timestamp = i64::deserialize(deserializer)?;
        let dt = chrono::DateTime::from_timestamp(timestamp, 0)
            .ok_or_else(|| serde::de::Error::custom("Invalid timestamp"))?;
        Ok(Timestamp(dt))
    }
}

// Using with helper functions
#[derive(Debug, Serialize, Deserialize)]
struct Task {
    id: String,
    #[serde(with = "chrono::serde::ts_seconds")]
    created_at: chrono::DateTime<chrono::Utc>,
    #[serde(serialize_with = "serialize_path")]
    #[serde(deserialize_with = "deserialize_path")]
    file_path: PathBuf,
}

fn serialize_path<S>(path: &PathBuf, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&path.to_string_lossy())
}

fn deserialize_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(PathBuf::from(s))
}

// Working with JSON
fn json_examples() -> Result<()> {
    let config = Config {
        name: "test".to_string(),
        version: "1.0".to_string(),
        enabled: true,
        description: Some("A test config".to_string()),
    };

    // Serialize to JSON string
    let json = serde_json::to_string(&config)?;
    let json_pretty = serde_json::to_string_pretty(&config)?;

    // Deserialize from JSON string
    let parsed: Config = serde_json::from_str(&json)?;

    // Work with generic JSON values
    let mut value: JsonValue = serde_json::from_str(&json)?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert("extra".to_string(), JsonValue::Bool(true));
    }

    // Convert to specific type
    let config2: Config = serde_json::from_value(value)?;

    Ok(())
}

// Working with TOML
fn toml_examples() -> Result<()> {
    let config = Config {
        name: "test".to_string(),
        version: "1.0".to_string(),
        enabled: true,
        description: None,
    };

    // Serialize to TOML
    let toml = toml::to_string(&config)?;
    let toml_pretty = toml::to_string_pretty(&config)?;

    // Deserialize from TOML
    let parsed: Config = toml::from_str(&toml)?;

    Ok(())
}

// Working with YAML
// WARNING: serde_yaml is DEPRECATED (archived by its author in 0.9.34, 2024-03-25; its
// yaml-rust dep is unmaintained, RUSTSEC-2024-0320). The serde_yml fork carries an
// unsoundness advisory (RUSTSEC-2025-0068, versions <= 0.0.12). For Rust/Cargo-native
// config prefer TOML or JSON. If YAML is required, use the maintained yaml-rust2 fork
// (or serde-saphyr built on it — note it does NOT implement serde's own traits, so it
// is not a drop-in serde replacement; verify the API before migrating).
// The serde_yaml calls below are shown only as legacy reference.
fn yaml_examples() -> Result<()> {
    let config = Config {
        name: "test".to_string(),
        version: "1.0".to_string(),
        enabled: true,
        description: None,
    };

    // Serialize to YAML (legacy serde_yaml; prefer TOML/JSON)
    let yaml = serde_yaml::to_string(&config)?;

    // Deserialize from YAML
    let parsed: Config = serde_yaml::from_str(&yaml)?;

    Ok(())
}

// Generic serialization function
fn serialize_any<T: Serialize>(value: &T, format: &str) -> Result<String> {
    match format {
        "json" => Ok(serde_json::to_string_pretty(value)?),
        "toml" => Ok(toml::to_string_pretty(value)?),
        "yaml" => Ok(serde_yaml::to_string(value)?),
        _ => Err(anyhow::anyhow!("Unsupported format: {}", format)),
    }
}
```

### Async/Await with Tokio

```rust
use tokio::sync::{mpsc, Mutex, RwLock};
use std::sync::Arc;
use tokio::time::{sleep, Duration, timeout};

// Basic async function
async fn fetch_data(url: &str) -> Result<String> {
    let response = reqwest::get(url).await?;
    let body = response.text().await?;
    Ok(body)
}

// Concurrent execution with join_all (fine for inline, non-spawned futures at small N).
// For SPAWNED tasks prefer tokio::task::JoinSet: it re-polls only the woken task
// (join_all re-polls all pending futures on every wakeup — O(N) at large N), yields
// results in completion order, aborts the rest on drop, and surfaces panics as JoinError.
async fn fetch_all(urls: Vec<String>) -> Vec<Result<String>> {
    let futures: Vec<_> = urls.iter().map(|url| fetch_data(url)).collect();
    futures::future::join_all(futures).await
}

// Select multiple futures - first to complete wins
async fn fetch_with_fallback(primary: &str, fallback: &str) -> Result<String> {
    tokio::select! {
        result = fetch_data(primary) => result,
        result = fetch_data(fallback) => result,
    }
}

// Timeout for async operations
async fn fetch_with_timeout(url: &str) -> Result<String> {
    timeout(Duration::from_secs(5), fetch_data(url))
        .await
        .context("Request timed out")?
}

// Shared state. Use std::sync::Mutex for state NOT held across an .await
// (cheaper than tokio's async mutex); reserve tokio::sync::Mutex/RwLock for
// guards that genuinely must span an await point (e.g. the cache below).
struct AppState {
    counter: Arc<std::sync::Mutex<u64>>,
    cache: Arc<RwLock<HashMap<String, String>>>, // tokio::sync::RwLock: held across .await
}

impl AppState {
    fn increment(&self) -> u64 {
        let mut counter = self.counter.lock().unwrap(); // std Mutex; guard dropped before any .await
        *counter += 1;
        *counter
    }

    // tokio RwLock for read-heavy workloads where the guard crosses .await
    async fn get_cached(&self, key: &str) -> Option<String> {
        let cache = self.cache.read().await;
        cache.get(key).cloned()
    }

    async fn update_cache(&self, key: String, value: String) {
        let mut cache = self.cache.write().await;
        cache.insert(key, value);
    }
}

// Channel communication patterns
async fn producer_consumer() {
    let (tx, mut rx) = mpsc::channel(32);

    // Producer task
    tokio::spawn(async move {
        for i in 0..10 {
            if tx.send(i).await.is_err() {
                break; // Receiver dropped
            }
        }
    });

    // Consumer
    while let Some(value) = rx.recv().await {
        println!("Received: {}", value);
    }
}

// Multiple producers with broadcast
use tokio::sync::broadcast;

async fn broadcast_example() {
    let (tx, mut rx1) = broadcast::channel(16);
    let mut rx2 = tx.subscribe();

    tokio::spawn(async move {
        tx.send("message").unwrap();
    });

    tokio::join!(
        async { println!("rx1: {:?}", rx1.recv().await) },
        async { println!("rx2: {:?}", rx2.recv().await) },
    );
}

// Oneshot for single-value communication
use tokio::sync::oneshot;

async fn compute_task() -> i32 {
    sleep(Duration::from_secs(1)).await;
    42
}

async fn oneshot_example() {
    let (tx, rx) = oneshot::channel();

    tokio::spawn(async move {
        let result = compute_task().await;
        let _ = tx.send(result);
    });

    match rx.await {
        Ok(value) => println!("Got: {}", value),
        Err(_) => println!("Sender dropped"),
    }
}

// Spawning blocking tasks
async fn cpu_intensive_work() -> Result<String> {
    tokio::task::spawn_blocking(|| {
        // CPU-intensive operation that would block async runtime
        std::thread::sleep(Duration::from_secs(2));
        "done".to_string()
    })
    .await
    .context("Task panicked")
}

// Tokio runtime setup
fn main() {
    // Multi-threaded runtime
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        fetch_data("http://example.com").await.ok();
    });

    // Or use the macro
    #[tokio::main]
    async fn main() {
        // async main function
    }

    // Single-threaded runtime for lightweight apps
    #[tokio::main(flavor = "current_thread")]
    async fn main() {
        // async main with single thread
    }
}
```

## Common Patterns

### Error Handling Patterns

```rust
use thiserror::Error;
use anyhow::{Context, Result, bail, ensure};
use std::io;

// Library error types with thiserror
#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Invalid syntax at line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },

    #[error("Unknown token: {0}")]
    UnknownToken(String),

    #[error("Expected {expected}, found {found}")]
    Unexpected { expected: String, found: String },
}

// Application error types with context
pub type AppResult<T> = Result<T, AppError>;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Parse error")]
    Parse(#[from] ParseError),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Stage not found: {0}")]
    StageNotFound(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// Error context chain
fn load_and_parse_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file: {}", path))?;

    let config: Config = toml::from_str(&content)
        .with_context(|| format!("Failed to parse TOML in {}", path))?;

    validate_config(&config)
        .context("Config validation failed")?;

    Ok(config)
}

// Early validation with ensure
fn validate_config(config: &Config) -> Result<()> {
    ensure!(!config.name.is_empty(), "Config name cannot be empty");
    ensure!(config.port > 0, "Port must be greater than 0");
    ensure!(config.port < 65536, "Port must be less than 65536");

    if config.stages.is_empty() {
        bail!("Config must have at least one stage");
    }

    Ok(())
}

// Option to Result conversion patterns
fn get_stage(id: &str) -> Result<Stage> {
    let stage = find_stage(id)
        .ok_or_else(|| AppError::StageNotFound(id.to_string()))?;
    Ok(stage)
}

// Result unwrapping strategies
fn result_handling_examples() {
    let result: Result<String> = fetch_data();

    // Propagate with ?
    let data = result?;

    // Provide default
    let data = result.unwrap_or_default();
    let data = result.unwrap_or_else(|| "fallback".to_string());

    // Convert error type
    let data = result.map_err(|e| AppError::Other(e))?;

    // Explicit handling
    let data = match result {
        Ok(d) => d,
        Err(e) => {
            log::error!("Failed to fetch: {}", e);
            return Err(e);
        }
    };

    // Inspect without consuming
    if let Err(ref e) = result {
        log::warn!("Warning: {}", e);
    }

    // Chain operations
    let processed = result
        .and_then(|data| parse(&data))
        .and_then(|parsed| validate(&parsed))
        .map(|validated| transform(validated))?;
}

// Multiple error sources
fn multiple_operations() -> Result<()> {
    let file1 = std::fs::read_to_string("file1.txt")
        .context("Failed to read file1")?;
    let file2 = std::fs::read_to_string("file2.txt")
        .context("Failed to read file2")?;

    let result = process(&file1, &file2)
        .context("Failed to process files")?;

    save_result(&result)
        .context("Failed to save result")?;

    Ok(())
}

// Collecting Results
fn process_all_files(paths: &[&str]) -> Result<Vec<Content>> {
    // Stop on first error
    paths.iter()
        .map(|path| load_file(path))
        .collect::<Result<Vec<_>>>()
}

fn process_all_files_partial(paths: &[&str]) -> Vec<Result<Content>> {
    // Continue on errors, return all results
    paths.iter()
        .map(|path| load_file(path))
        .collect()
}

fn process_all_files_separate(paths: &[&str]) -> (Vec<Content>, Vec<anyhow::Error>) {
    // Separate successes from failures
    let results: Vec<_> = paths.iter()
        .map(|path| load_file(path))
        .collect();

    let mut successes = Vec::new();
    let mut failures = Vec::new();

    for result in results {
        match result {
            Ok(content) => successes.push(content),
            Err(e) => failures.push(e),
        }
    }

    (successes, failures)
}
```

### Builder Pattern

```rust
#[derive(Default)]
pub struct RequestBuilder {
    url: Option<String>,
    method: Method,
    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
    timeout: Duration,
}

impl RequestBuilder {
    pub fn new() -> Self {
        Self {
            method: Method::GET,
            timeout: Duration::from_secs(30),
            ..Default::default()
        }
    }

    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn method(mut self, method: Method) -> Self {
        self.method = method;
        self
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn build(self) -> Result<Request, BuildError> {
        let url = self.url.ok_or(BuildError::MissingUrl)?;
        Ok(Request {
            url,
            method: self.method,
            headers: self.headers,
            body: self.body,
            timeout: self.timeout,
        })
    }
}
```

### Newtype Pattern

```rust
// Type safety through newtype
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(String);

impl UserId {
    pub fn new(id: impl Into<String>) -> Self {
        UserId(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OrderId(String);

// Now these are different types - can't mix them up
fn get_user(id: UserId) -> Option<User> { /* ... */ }
fn get_order(id: OrderId) -> Option<Order> { /* ... */ }
```

### Smart Pointers and Interior Mutability

```rust
use std::rc::Rc;
use std::sync::{Arc, Mutex, RwLock};
use std::cell::{RefCell, Cell};

// Box - heap allocation, single owner
fn box_example() {
    // Large value on heap instead of stack
    let large_data = Box::new([0u8; 10000]);

    // Recursive types require Box
    enum List {
        Cons(i32, Box<List>),
        Nil,
    }
}

// Rc - shared ownership (single-threaded)
fn rc_example() {
    let shared = Rc::new(vec![1, 2, 3]);
    let ref1 = Rc::clone(&shared);
    let ref2 = Rc::clone(&shared);

    println!("Reference count: {}", Rc::strong_count(&shared)); // 3

    // Check before cloning
    if Rc::strong_count(&shared) < 10 {
        let ref3 = Rc::clone(&shared);
    }
}

// Arc - shared ownership (multi-threaded)
fn arc_example() {
    let shared = Arc::new(vec![1, 2, 3]);

    let handles: Vec<_> = (0..5)
        .map(|i| {
            let data = Arc::clone(&shared);
            std::thread::spawn(move || {
                println!("Thread {}: {:?}", i, data);
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

// RefCell - interior mutability (single-threaded)
fn refcell_example() {
    let data = RefCell::new(vec![1, 2, 3]);

    // Multiple immutable borrows
    {
        let borrow1 = data.borrow();
        let borrow2 = data.borrow();
        println!("{:?}", borrow1);
    } // Borrows dropped here

    // Mutable borrow
    data.borrow_mut().push(4);

    // try_borrow for runtime check
    match data.try_borrow_mut() {
        Ok(mut b) => b.push(5),
        Err(_) => println!("Already borrowed"),
    }
}

// Cell - interior mutability for Copy types
fn cell_example() {
    let counter = Cell::new(0);

    let increment = || {
        let current = counter.get();
        counter.set(current + 1);
    };

    increment();
    increment();
    assert_eq!(counter.get(), 2);
}

// Arc<Mutex<T>> - shared mutable state (multi-threaded)
fn arc_mutex_example() {
    let counter = Arc::new(Mutex::new(0));
    let mut handles = vec![];

    for _ in 0..10 {
        let counter = Arc::clone(&counter);
        let handle = std::thread::spawn(move || {
            let mut num = counter.lock().unwrap();
            *num += 1;
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }

    println!("Result: {}", *counter.lock().unwrap());
}

// Arc<RwLock<T>> - read-heavy workloads
fn arc_rwlock_example() {
    let cache = Arc::new(RwLock::new(HashMap::new()));

    // Multiple readers
    let readers: Vec<_> = (0..5)
        .map(|i| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                let cache = cache.read().unwrap();
                if let Some(value) = cache.get(&i) {
                    println!("Read: {}", value);
                }
            })
        })
        .collect();

    // Single writer
    let writer = {
        let cache = Arc::clone(&cache);
        std::thread::spawn(move || {
            let mut cache = cache.write().unwrap();
            cache.insert(0, "value".to_string());
        })
    };

    for reader in readers {
        reader.join().unwrap();
    }
    writer.join().unwrap();
}

// Rc<RefCell<T>> - shared mutable state (single-threaded)
struct Node {
    value: i32,
    children: Vec<Rc<RefCell<Node>>>,
}

fn rc_refcell_tree() {
    let root = Rc::new(RefCell::new(Node {
        value: 1,
        children: vec![],
    }));

    let child = Rc::new(RefCell::new(Node {
        value: 2,
        children: vec![],
    }));

    root.borrow_mut().children.push(Rc::clone(&child));

    // Modify child through shared reference
    child.borrow_mut().value = 3;
}

// Weak references to prevent cycles
use std::rc::Weak;

struct Parent {
    children: Vec<Rc<RefCell<Child>>>,
}

struct Child {
    parent: Weak<RefCell<Parent>>,
}

fn weak_reference_example() {
    let parent = Rc::new(RefCell::new(Parent { children: vec![] }));

    let child = Rc::new(RefCell::new(Child {
        parent: Rc::downgrade(&parent),
    }));

    parent.borrow_mut().children.push(Rc::clone(&child));

    // Access parent from child
    if let Some(parent) = child.borrow().parent.upgrade() {
        println!("Parent exists");
    }
}

// Pattern: Interior mutability for caching
struct Database {
    cache: RefCell<HashMap<String, String>>,
}

impl Database {
    fn get(&self, key: &str) -> Option<String> {
        // Check cache with immutable self
        if let Some(value) = self.cache.borrow().get(key) {
            return Some(value.clone());
        }

        // Fetch from database
        let value = self.fetch_from_db(key)?;

        // Update cache with immutable self
        self.cache.borrow_mut().insert(key.to_string(), value.clone());

        Some(value)
    }

    fn fetch_from_db(&self, key: &str) -> Option<String> {
        // Database query
        None
    }
}
```

### Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic() {
        let result = add(2, 3);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_with_result() -> Result<(), Box<dyn Error>> {
        let config = parse_config("valid config")?;
        assert_eq!(config.name, "test");
        Ok(())
    }

    #[test]
    #[should_panic(expected = "divide by zero")]
    fn test_panic() {
        divide(1, 0);
    }

    // Async tests with tokio
    #[tokio::test]
    async fn test_async_function() {
        let result = fetch_data("http://example.com").await;
        assert!(result.is_ok());
    }

    // Property-based testing with proptest
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_parse_roundtrip(s in "[a-z]+") {
            let parsed = parse(&s)?;
            let serialized = serialize(&parsed);
            prop_assert_eq!(s, serialized);
        }
    }
}
```

## Anti-Patterns

### Avoid These Practices

```rust
// BAD: Unnecessary clone
fn process(items: &Vec<String>) {
    for item in items.clone() {  // Unnecessary allocation
        println!("{}", item);
    }
}

// GOOD: Iterate by reference
fn process(items: &[String]) {
    for item in items {
        println!("{}", item);
    }
}

// BAD: Using unwrap/expect in library code
fn parse_config(s: &str) -> Config {
    serde_json::from_str(s).unwrap()  // Panics on invalid input
}

// GOOD: Return Result and let caller handle errors
fn parse_config(s: &str) -> Result<Config, serde_json::Error> {
    serde_json::from_str(s)
}

// BAD: Excessive use of Rc<RefCell<T>>
struct Node {
    value: i32,
    children: Vec<Rc<RefCell<Node>>>,
}

// GOOD: Consider arena allocation or indices
struct Arena {
    nodes: Vec<Node>,
}
struct Node {
    value: i32,
    children: Vec<usize>,  // Indices into arena
}

// BAD: String concatenation in loops
fn build_message(parts: &[&str]) -> String {
    let mut result = String::new();
    for part in parts {
        result = result + part + ", ";  // Creates new String each iteration
    }
    result
}

// GOOD: Use push_str or collect
fn build_message(parts: &[&str]) -> String {
    parts.join(", ")
}

// BAD: Boxing errors unnecessarily
fn parse(s: &str) -> Result<Data, Box<dyn Error>> {
    // For libraries, use concrete error types
}

// GOOD: Use concrete error types in libraries
fn parse(s: &str) -> Result<Data, ParseError> {
    // thiserror for library errors, anyhow for applications
}

// BAD: Unsafe without justification
unsafe fn get_unchecked(slice: &[i32], index: usize) -> i32 {
    *slice.get_unchecked(index)
}

// GOOD: Safe by default, unsafe with clear invariants
fn get_unchecked(slice: &[i32], index: usize) -> i32 {
    // SAFETY: Caller must ensure index < slice.len()
    // Only use when bounds checking is a proven bottleneck
    debug_assert!(index < slice.len());
    unsafe { *slice.get_unchecked(index) }
}

// BAD: Ignoring must_use
let _ = fs::remove_file("temp.txt");  // Error silently ignored

// GOOD: Handle the result
fs::remove_file("temp.txt").ok();  // Explicitly ignore
// or
fs::remove_file("temp.txt")?;  // Propagate error
```

### Quick Pattern Swaps

```rust
// BAD: Locking callers into Vec and String
fn print_names(names: &Vec<String>, title: &String) {
    println!("{title}");
    for name in names {
        println!("{name}");
    }
}

// GOOD: Accept slices and str
fn print_names(names: &[String], title: &str) {
    println!("{title}");
    for name in names {
        println!("{name}");
    }
}

// BAD: Matching only to re-return the same error
fn read_config(path: &str) -> Result<String, std::io::Error> {
    let config = match std::fs::read_to_string(path) {
        Ok(config) => config,
        Err(err) => return Err(err),
    };
    Ok(config)
}

// GOOD: Use ? to propagate unchanged errors
fn read_config(path: &str) -> Result<String, std::io::Error> {
    let config = std::fs::read_to_string(path)?;
    Ok(config)
}
```

## Expert Practices: Idioms, Anti-Patterns & Gotchas

High-signal guidance an experienced Rust engineer applies reflexively. Each item states the *why* (mechanism), not just the rule.

### Async & Concurrency Gotchas

**Never hold a `std::sync::Mutex` guard across an `.await`.** `std::sync::MutexGuard` is `!Send`, so a future holding one across an await point is itself `!Send` and `tokio::spawn` on the multi-thread runtime rejects it with a compile error. Worse, when the future is *not* sent (single task / `current_thread`), it compiles but the worker thread blocks on the lock and can never poll the task that would release it — a runtime deadlock the compiler does NOT catch. Default to `std::sync::Mutex` and drop the guard in an explicit `{ }` scope before any `.await`; reach for `tokio::sync::Mutex` (whose guard *is* `Send`, at the cost of async locking) only when the guard genuinely must span an await.

```rust
use std::sync::Mutex;
async fn update(state: &Mutex<Vec<u32>>, val: u32) {
    {
        let mut guard = state.lock().unwrap();
        guard.push(val);
    } // guard dropped HERE, before any await
    some_async_operation().await;
}
```

**`tokio::select!` cancels (drops) losing branches at their `.await` — most async ops are NOT cancel-safe.** When one branch completes, every other branch future is dropped at its suspension point and any state in its local variables is silently lost; in a loop the loser is discarded each iteration, with no warning. NOT cancel-safe: `read_exact`, `read_to_end`, `write_all`, `Mutex::lock`, `RwLock::read/write`, `Semaphore::acquire`. Cancel-safe: `mpsc::Receiver::recv`, `TcpListener::accept`, `AsyncReadExt::read` (returns partial). Put only cancel-safe operations directly in `select!` branches, or store resumable state in a struct field rather than a future-local.

```rust
// cancel-safe: recv drops cleanly if the other branch fires
tokio::select! {
    msg = rx.recv() => { /* handle */ }
    _ = shutdown.cancelled() => return,
}
```

**`tokio::spawn` requires `Send + 'static` — `Rc`, `RefCell`, and borrowed locals silently break it.** The future must own all its data (no borrows of the enclosing scope) and every value live across an `.await` must be `Send`. `Rc`/`RefCell` are `!Send`; use `Arc` (and a `Mutex` for shared mutation). Use `async move { }` to move owned data in. Subtle trap: a non-`Send` value created and dropped within an await-free span can still poison `Send` inference because auto-trait analysis spans the whole async block — force an early drop with an explicit `{ }` scope.

**`spawn_blocking` is for blocking I/O, not CPU-bound work at scale.** It runs on a separate blocking-thread pool (default max 512, via `Builder::max_blocking_threads`), distinct from the async worker pool, meant for short-lived blocking I/O. Using it for CPU computation at scale is a trap: past the limit further calls silently queue, a running blocking task cannot be aborted, and a queued one only *may* be cancelled. For CPU work, use `rayon` and bridge back via a `oneshot` channel.

```rust
async fn compress_async(data: Vec<u8>) -> Vec<u8> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rayon::spawn(move || { let _ = tx.send(compress(&data)); });
    rx.await.expect("rayon task dropped")
}
```

**Prefer `tokio::task::JoinSet` over `futures::join_all` for spawned concurrent tasks.** `join_all` polls a collection of inline (non-spawned) futures in one task — fine for small N, but it re-polls *all* pending futures on any wakeup (O(N)) and does not surface panics cleanly. `JoinSet` spawns onto the scheduler (only the woken task re-polls), yields results in completion order via `join_next`, aborts remaining tasks on drop, and reports panics as a `JoinError`.

```rust
let mut set = tokio::task::JoinSet::new();
for url in urls { set.spawn(fetch_data(url)); }
while let Some(res) = set.join_next().await {
    match res {
        Ok(Ok(data)) => process(data),
        Ok(Err(e)) => eprintln!("fetch error: {e}"),
        Err(e) => eprintln!("task panicked: {e}"),
    }
}
```

### Gotchas (Silent Footguns)

**`let _ = guard` drops immediately and gives zero protection.** `let _ = expr` does NOT bind — it drops the value at end of statement. `let _ = mutex.lock().unwrap();` acquires and instantly releases the lock. Bind a name (`let _guard = ...`) to hold until end of scope, or `drop(guard)` to release explicitly. The near-identical `let _name = ...` behaves completely differently (it binds). Inside `move` closures, `let _ = captured_var` may not even capture the variable.

**Struct fields drop in declaration order; locals drop in reverse.** Drop is deterministic but asymmetric: struct fields drop FORWARD (declaration order), `let` locals drop REVERSE (LIFO); tuples/arrays drop forward; a struct's own `Drop::drop()` runs BEFORE its fields. So the field that must outlive the others (e.g. a pool a handle borrows from) is declared LAST. This matters only when a field's `Drop` actually uses another field.

```rust
struct Worker {
    handle: Handle, // declared first -> dropped first (uses pool)
    pool: Pool,     // declared last  -> dropped last (still alive when handle drops)
}
```

**Integer arithmetic panics on overflow in debug but wraps silently in release.** Plain `+ - *` panic in debug builds but perform two's-complement wrapping under `--release`, with no warning about the difference — a reliable debug panic becomes silent corruption in production. Use the explicit families on untrusted/user-controlled sizes: `checked_*` (`Option`), `wrapping_*` (always wraps), `saturating_*` (clamps), `overflowing_*` (returns `(value, bool)`).

**`HashMap` iteration order is randomized per run — never assert on it.** The default `RandomState`/SipHash hasher reseeds per run (HashDoS resistance), so iteration order varies between program runs. Code asserting a stable sequence — tests, snapshots, serialization, logs — fails intermittently (often passing locally, failing in CI). For deterministic order, sort a collected `Vec`, use `BTreeMap` (key-ordered), or `indexmap::IndexMap` (insertion order).

**`std::process::exit` skips all `Drop` impls and does not flush Rust I/O buffers.** It runs C `atexit` handlers but NO Rust destructors — lock-file removal, temp-file cleanup, connection teardown are abandoned, and buffered stdout/stderr may be lost. Return from `main` (`Result` or `ExitCode`) instead, which runs all destructors and flushes. `std::process::abort()` is more aggressive still (skips `atexit` too).

```rust
use std::process::ExitCode;
fn main() -> ExitCode {
    if !setup() { return ExitCode::FAILURE; } // destructors still run
    ExitCode::SUCCESS
}
```

**`#[serde(untagged)]` silently matches the first overlapping variant.** It tries variants in declaration order and returns the FIRST that deserializes, with no ambiguity error; an earlier variant with an overlapping shape silently wins. Errors are unhelpful (`data did not match any variant`), and it is slow in index-based formats like bincode. Prefer externally tagged (the default), internally tagged (`tag = "..."`), or adjacently tagged. If untagged is unavoidable, order variants most-specific first and add per-variant round-trip tests.

### Idioms & API Design

**Library code returns `Result` for expected failures; `panic!` only for violated invariants.** `panic!` removes the caller's ability to recover — appropriate only when a caller-upheld invariant is broken (a bug, like out-of-bounds indexing), when continuing is unsafe, or for a truly impossible branch. All expected failure modes (I/O errors, malformed input, missing resources) return `Result`. Corollary: encode validity in the type system via newtypes/constructors so validation happens once at construction.

**Implement `From`, never `Into` — and accept `Into<T>` in generic bounds.** std's blanket `impl<T, U: From<T>> Into<U> for T` means every `From` impl yields `Into` for free; implementing `Into` manually does NOT grant the reverse `From`. Implement `From` for conversions; accept `Into<T>` in bounds for the widest caller flexibility. (Direct `Into` impls are essentially only for the pre-1.41 orphan-rule edge case.)

**Never implement `ToString` — implement `Display`.** std ships `impl<T: Display + ?Sized> ToString for T`, so `to_string()` routes through `Display` automatically; a user blanket `impl ToString` conflicts with std's and violates the orphan rule (won't compile). (See the corrected Traits and Generics example above.)

**Name conversions by cost and ownership; never prefix getters with `get_`.** Per the API Guidelines (C-CONV, C-GETTER): `as_` is a free borrowed-to-borrowed reinterpret (`str::as_bytes`); `to_` is an expensive, possibly-allocating conversion (`str::to_uppercase`); `into_` is a consuming owned-to-owned conversion (`String::into_bytes`). Getters take the field name with no `get_` prefix (`first()`, not `get_first()`); reserve bare `get()` for a single obvious value (`Cell::get`).

**Use `.flatten()` instead of `.filter_map(|x| x)`.** For an iterator of `Option<T>`, `.flatten()` clearly drops the `None`s; the identity closure form is flagged by `clippy::filter_map_identity` — a build failure under `clippy -- -D warnings`. (`filter_map` with a *non-identity* closure is itself idiomatic.)

### Design Patterns & Performance

**Return `Cow<'_, B>` to borrow in the common case and allocate only when the value must change.** When a function returns its input unchanged on the common path but a freshly-allocated value otherwise (escaping, normalization, validation), `Cow` avoids forcing an allocation on the unchanged path. Read-only callers deref for free; callers needing ownership call `into_owned()` (at most one allocation either way).

```rust
use std::borrow::Cow;
fn html_escape(s: &str) -> Cow<'_, str> {
    if s.contains(['<', '>', '&']) {
        Cow::Owned(s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"))
    } else {
        Cow::Borrowed(s) // zero allocation in the common case
    }
}
```

**`impl Trait`/generics (static dispatch) vs `dyn Trait` (dynamic dispatch).** `impl Trait` / generics monomorphize — one copy per concrete type, enabling inlining but bloating code if instantiated widely; use on hot paths where types are known at compile time. `dyn Trait` dispatches through a vtable (indirect call, no inlining) but yields one code path and enables heterogeneous collections / runtime plugin dispatch — use for `Vec<Box<dyn Stage>>` and similar. In return position, RPIT (`impl Trait`) avoids boxing but fixes one concrete type; `Box<dyn Trait>` allows runtime variation at an allocation.

**Implement `Deref`/`DerefMut` only for smart pointers.** Deref coercion is implicit and transitive; the std docs say implement it "only for smart pointers to avoid confusion." On a plain wrapper it silently exposes every method of the target (unpredictable API surface, name collisions) and can pass through multiple layers. For wrapper types, write explicit delegation methods instead.

**`PhantomData` carries variance, ownership, and auto-trait propagation for types holding raw pointers.** A struct with raw pointers gives the compiler no info about variance, `Send`/`Sync`, or drop-check (`*const T`/`*mut T` are `!Send + !Sync`, fixed variance). `PhantomData<X>` is a zero-size marker saying "acts as if it stores an `X`": `PhantomData<&'a T>` is covariant + `Send if T: Sync`; `PhantomData<&'a mut T>` is invariant; `PhantomData<fn(T)>` is contravariant. The wrong marker is a subtle unsoundness — consult the Nomicon table.

### Security & `unsafe`

**`unsafe impl Send`/`Sync` is a soundness promise the compiler cannot check.** It asserts you manually verified thread-safety. Two commonly-missed invariants: (1) if `T: Drop`, its destructor must be safe to run on ANY thread (why std `MutexGuard` is `!Send` on POSIX — the mutex must be released on the acquiring thread); (2) raw pointers make a type `!Send + !Sync`, so wrapping them needs an explicit `unsafe impl` plus a proof that all accesses are synchronized.

```rust
struct SharedPtr(*mut u32);
unsafe impl Send for SharedPtr {} // UNSOUND: races on the pointer, no synchronization
```

**Panicking across an FFI boundary is UB — `catch_unwind` at every `extern "C"` entry.** Unwinding into or out of Rust across FFI is undefined behavior. For Rust functions called from C, wrap the body in `std::panic::catch_unwind` and convert the `Result` to an error code. As of Rust 1.71+, a panic escaping an `extern "C"` fn aborts the process (safe but a silent crash) rather than unwinding, so `catch_unwind` is still needed to return control to C. Use the `extern "C-unwind"` ABI only when both sides support unwinding.

```rust
#[no_mangle]
pub extern "C" fn rust_process(data: *const u8, len: usize) -> i32 {
    let result = std::panic::catch_unwind(|| {
        let slice = unsafe { std::slice::from_raw_parts(data, len) };
        process(slice)
    });
    match result { Ok(v) => v, Err(_) => -1 }
}
```

**References to `static mut` are a deny-by-default error in Rust 2024.** Aliasing a `static mut` can violate Rust's aliasing rules with no borrow-site check. Use `std::sync::OnceLock<T>` for lazily-initialized read-after-init globals, `Mutex<T>`/`RwLock<T>` for mutable shared state, and `&raw mut S` / `std::ptr::addr_of_mut!(S)` for low-level/FFI raw access. (`SyncUnsafeCell` is still nightly-only — do not rely on it.)

```rust
use std::sync::OnceLock;
static CONFIG: OnceLock<Config> = OnceLock::new();
fn get_config() -> &'static Config { CONFIG.get_or_init(Config::load) }
```

### Currency (Modern Rust & Crate Notes)

**`async fn` in traits (AFIT) is stable since 1.75 but NOT dyn-compatible.** You cannot form `dyn MyTrait`. The "Send bound problem": the associated future has no `Send` bound, so `tokio::spawn` consumers hit "future cannot be sent between threads" and cannot add the bound retroactively. Don't bake `+ Send` into the `async fn` (that breaks single-threaded users); use the `#[trait_variant::make(NameSend: Send)]` macro from the rust-lang `trait-variant` crate to generate both a plain and a `Send` variant.

```rust
#[trait_variant::make(FetcherSend: Send)]
pub trait Fetcher {
    async fn fetch(&self, url: &str) -> Result<String, Error>;
}
```

**Rust 2024 RPIT captures all in-scope generics (including lifetimes) by default.** In 2021 and earlier, return-position `impl Trait` captured only type parameters, forcing `+ '_`/`+ 'a` workarounds. In 2024 (stable in 1.85.0) the hidden type captures ALL in-scope generics including lifetimes. The `use<..>` precise-capture bound (stabilized 1.82) lets you opt OUT of capturing a lifetime when the return value does not borrow the input.

```rust
fn first_word(s: &str) -> impl std::fmt::Display + use<'_> { s.split_whitespace().next().unwrap_or("") }
fn indices<T>(slice: &[T]) -> impl Iterator<Item = usize> + use<T> { 0..slice.len() } // opt out of borrowing slice
```

**Async closures (`async || {}`) and `AsyncFn`/`AsyncFnMut`/`AsyncFnOnce` are stable since 1.85.** Unlike `|args| async move { ... }`, a true async closure can borrow from its captured environment across await points, removing forced `Arc`/clone/move workarounds; the new traits replace ad-hoc `Box<dyn Fn(..) -> Pin<Box<dyn Future>>>` bounds for accepting async callbacks.

**Let chains (`if let ... && ...`) are stable in edition 2024 since 1.88.** Mix let-pattern bindings and boolean expressions in `if`/`while` with `&&`, flattening nested `if let` and removing intermediate `Option`/`Result` juggling. Requires `edition = "2024"`.

```rust
if let Some(user) = get_user(id)
    && user.is_active()
    && let Some(email) = user.email.as_ref()
{
    send_email(email);
}
```

**Use `std::pin::pin!` instead of `tokio::pin!` / `pin_utils::pin_mut!`.** Stable since 1.68, `std::pin::pin!` pins a value to its local frame without heap allocation, yielding `Pin<&mut T>`. A value pinned with `pin!()` cannot be returned out of its scope (it borrows a local) — use `Box::pin` for that.

**`thiserror` 2.0 (Nov 2024): must be a direct dependency.** Any crate using `#[derive(Error)]` must declare `thiserror` as a DIRECT dependency (not just transitive); format strings no longer accept raw identifiers (use `{type}`, not `{r#type}`); trait bounds are no longer inferred on fields shadowed by explicit format args. New: `no_std` via `default-features = false`, out-of-line `#[error(fmt = path::to::fn)]`, and per-variant `#[error(transparent)]`. Pin `thiserror = "2"`.

**Centralize lint policy with `[workspace.lints]` (stable since 1.74).** Root `[workspace.lints.{rust,clippy}]` plus `[lints] workspace = true` in each member enforces lint policy across all crates without per-crate `#![deny]` attributes. Combine with `[workspace.package]` (inherit edition/rust-version) and `[workspace.dependencies]` (pin versions). (See the Workspace Structure example above.)
