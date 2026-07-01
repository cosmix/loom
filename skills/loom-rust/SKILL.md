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

Idiomatic, production-quality Rust for an engineer who already knows ownership/borrowing. This skill is decision rules, gotchas, and the traps that cost hours: async/`Send` correctness, error-handling architecture, serde pitfalls, and edition-2024 features. Assumes the borrow checker itself is not the problem — using it *well* is.

## Error Handling: anyhow vs thiserror

**The decision rule:** will a caller ever `match` on the error to recover? → `thiserror` (concrete, matchable enum). Does the error only bubble up to be logged/reported? → `anyhow` (type-erased, context chains). **Never put `anyhow::Error` in a library's public API** — it erases the type and denies callers any recovery. Libraries expose `thiserror` enums; binaries/apps consume with `anyhow`.

```rust
// Library boundary: thiserror. Each variant = a distinct, matchable failure mode.
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error), // #[from] gives `?` conversion for free
    #[error("invalid syntax at {line}:{column}: {message}")]
    Syntax { line: usize, column: usize, message: String },
    #[error("unexpected token: expected {expected}, found {found}")]
    Unexpected { expected: String, found: String },
    #[error(transparent)] // wrap another error without adding a layer
    Other(#[from] anyhow::Error),
}
```

```rust
// Application code: anyhow. `?` unifies heterogeneous errors; .context adds a chain.
use anyhow::{Context, Result, bail, ensure};

fn load_config(path: &str) -> Result<Config> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("reading config: {path}"))?; // with_context: lazy, use for allocating msgs
    let config: Config = toml::from_str(&content).context("parsing config TOML")?; // context: eager literal
    ensure!(!config.name.is_empty(), "config name cannot be empty"); // returns Err on false
    if config.port == 0 { bail!("port must be non-zero"); }          // early return with Err
    Ok(config)
}
```

- `context` (eager, takes a value) vs `with_context(|| ...)` (lazy closure) — use `with_context` whenever the message allocates (`format!`), else you pay the cost on the success path too.
- `#[from]` generates `From` for `?`; `#[error(transparent)]` forwards `Display`/`source` to the inner error (use for a pass-through variant).
- Option→Result: `.ok_or_else(|| Error::NotFound(id.to_string()))?`. Prefer `ok_or_else` (lazy) over `ok_or` when the error allocates.
- Collecting: `iter.map(f).collect::<Result<Vec<_>>>()` stops at the first `Err`; collect into `Vec<Result<_>>` to keep all outcomes.
- **`thiserror` 2.0:** must be a **direct** dependency (not transitive); format strings dropped raw-identifier support (`{type}`, not `{r#type}`); field trait bounds no longer inferred when shadowed by a format arg. New: `no_std` via `default-features = false`, out-of-line `#[error(fmt = path)]`, per-variant `#[error(transparent)]`. Pin `thiserror = "2"`.

## Traits, Generics & Lifetimes

```rust
// Accept the widest input, return the narrowest: Into<T> in bounds, From impls only.
// std's blanket `impl<T, U: From<T>> Into<U> for T` means every From yields Into for free;
// implementing Into manually does NOT grant the reverse From. So implement From, bound on Into.
fn store(&mut self, id: impl Into<String>) { self.id = id.into(); }

// where-clauses for readability on complex bounds
fn merge<V>(a: impl IntoIterator<Item = V>, b: impl IntoIterator<Item = V>) -> Vec<V>
where V: Ord {
    let mut v: Vec<V> = a.into_iter().chain(b).collect();
    v.sort();
    v.dedup();
    v
}

// Blanket impls are legal ONLY when the trait or the type is local to your crate (orphan rule).
trait Summary { fn summary(&self) -> String; }
impl<T: std::fmt::Display> Summary for T { fn summary(&self) -> String { self.to_string() } }
```

- **Never implement `ToString`** — std ships `impl<T: Display + ?Sized> ToString for T`. A manual impl conflicts with std's and violates the orphan rule (E0119, won't compile). Implement `Display`; `to_string()` comes free.
- **`impl Trait`/generics (static) vs `dyn Trait` (dynamic):** generics monomorphize — inlinable, but N copies bloat the binary if instantiated widely. `dyn Trait` is a vtable indirect call (no inlining) but one code path and heterogeneous collections (`Vec<Box<dyn Stage>>`). RPIT (`-> impl Trait`) avoids boxing but fixes one concrete type; `Box<dyn Trait>` allows runtime variation at an allocation.
- **Implement `Deref`/`DerefMut` only for smart pointers.** Deref coercion is implicit and transitive; on a plain wrapper it silently exposes the target's entire API (name collisions, unpredictable surface). Write explicit delegation methods for wrappers.
- **Name conversions by cost (API Guidelines C-CONV/C-GETTER):** `as_` = free borrow reinterpret (`str::as_bytes`); `to_` = expensive/allocating (`str::to_uppercase`); `into_` = consuming owned→owned (`String::into_bytes`). Getters take the field name with **no** `get_` prefix (`first()`, not `get_first()`).
- **Lifetime elision:** `fn get(&self, k: &str) -> Option<&str>` desugars to output borrowing `&self`. Only annotate when multiple input lifetimes make the output ambiguous.
- **Borrow-checker fights:** to mutate two struct fields at once, **destructure** rather than call `&mut self` methods: `let Self { a, b } = self;` gives independent borrows. For slices, `split_at_mut`. See also the `entry` API below.

## Iterators

Prefer iterator chains over index loops: no bounds checks, no off-by-one, and the optimizer fuses them into tight loops.

```rust
let emails: Vec<String> = users.into_iter()
    .filter(|u| u.active)
    .filter_map(|u| u.email)          // yields T from Option<T>, dropping None
    .collect();

let (evens, odds): (Vec<_>, Vec<_>) = nums.iter().partition(|&&x| x % 2 == 0);
let sum: i32 = nums.iter().sum();
let first_even = nums.iter().find(|&&x| x % 2 == 0);
```

- Use `.flatten()` on an iterator of `Option`/`Result`, not `.filter_map(|x| x)` — the identity closure is a `clippy::filter_map_identity` error under `-D warnings`. (`filter_map` with a *real* closure is idiomatic.)
- `.collect::<Result<Vec<_>>>()` short-circuits on the first `Err`; `.collect::<Vec<Result<_>>>()` keeps every outcome.
- Reach for `itertools` (`.chunks`, `.group_by`, `.dedup_by`, `.sorted`, `.unique`) before hand-rolling; `.try_fold` for early-exit accumulation.
- Custom iterators: implement `Iterator::next` returning `Option<Self::Item>` — you get `map`/`filter`/`collect` etc. for free.

## Serde

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    name: String,
    #[serde(default)] enabled: bool,                       // missing → Default
    #[serde(default = "default_timeout")] timeout: u64,    // missing → fn value
    #[serde(skip_serializing_if = "Option::is_none")] description: Option<String>,
    #[serde(skip)] runtime_state: Option<String>,          // never (de)serialized
    #[serde(rename = "userId")] user_id: String,
    #[serde(flatten)] extra: std::collections::HashMap<String, String>, // absorbs unknown keys
}
fn default_timeout() -> u64 { 30 }

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]   // enum variants / fields in one shot
enum Status { Active, Inactive, Pending }
```

**Enum tagging — pick deliberately, it defines the wire format:**

| Representation | Attribute | Wire shape |
| --- | --- | --- |
| Externally tagged (default) | *(none)* | `{"Text": {"content": "..."}}` |
| Internally tagged | `#[serde(tag = "type")]` | `{"type": "Text", "content": "..."}` |
| Adjacently tagged | `#[serde(tag = "t", content = "c")]` | `{"t": "Text", "c": {...}}` |
| Untagged | `#[serde(untagged)]` | bare value, variant inferred |

⚠ **`#[serde(untagged)]` silently matches the FIRST variant that deserializes** — declaration order is load-bearing. An earlier variant with an overlapping shape wins with no ambiguity error; failures produce the useless `data did not match any variant`; it is slow in index formats (bincode). Order variants **most-specific first** and add per-variant round-trip tests. Internally tagged also can't represent newtype-of-non-struct variants. Prefer a real tag whenever you control the format.

- Custom (de)serialize: `#[serde(with = "module")]`, or `serialize_with`/`deserialize_with` per field; whole-type via `impl Serialize`/`Deserialize`. `chrono` ships helpers (`chrono::serde::ts_seconds`).
- **`serde_yaml` is DEPRECATED** (archived at 0.9.34, 2024-03-25; its `yaml-rust` dep is RUSTSEC-2024-0320). The `serde_yml` fork has an unsoundness advisory (RUSTSEC-2025-0068, ≤0.0.12). Prefer TOML/JSON for Rust-native config; if YAML is mandatory use the maintained `yaml-rust2`, and verify the API (`serde-saphyr` does not implement serde's traits — not a drop-in).

## Async & Concurrency (Tokio)

```rust
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

// std::sync::Mutex for state NOT held across .await (cheaper); tokio's async Mutex/RwLock
// ONLY when the guard genuinely must span an await point.
struct AppState {
    counter: Arc<std::sync::Mutex<u64>>,               // never crosses .await
    cache: Arc<RwLock<std::collections::HashMap<String, String>>>, // crosses .await
}

impl AppState {
    fn increment(&self) -> u64 {
        let mut c = self.counter.lock().unwrap();  // std guard, dropped before any await
        *c += 1;
        *c
    }
    async fn get_cached(&self, key: &str) -> Option<String> {
        self.cache.read().await.get(key).cloned()  // tokio RwLock held across await
    }
}
```

⚠ **Never hold a `std::sync::MutexGuard` across an `.await`.** The guard is `!Send`, so a future holding one is `!Send` and `tokio::spawn` on the multi-thread runtime rejects it at compile time. Worse: on a single task / `current_thread` it *compiles* but the worker blocks on the lock and can never poll the task that would release it — a **runtime deadlock the compiler does not catch**. Drop the guard in an explicit `{ }` scope before the await, or use `tokio::sync::Mutex` (guard is `Send`, at the cost of async locking) only when it must span the await.

⚠ **The same trap with `RefCell`:** a `Ref`/`RefMut` held across `.await` can hit a `BorrowMutError` **panic** when another task on the runtime re-enters the same cell. `RefCell` is `!Send` so it can't cross into a multi-thread `spawn` anyway — drop the borrow before awaiting.

⚠ **`tokio::spawn` requires `Send + 'static`.** The future must OWN its data (`async move { }`) — borrowed locals, `Rc`, `RefCell` break it. Subtle: a non-`Send` value created *and dropped* within an await-free span can still poison `Send` inference (auto-trait analysis spans the whole async block) — force an early drop with an explicit `{ }` scope.

⚠ **`tokio::select!` drops (cancels) losing branches at their suspension point** — most async ops are NOT cancel-safe, and any state in the loser's future-local vars is silently lost each loop iteration.

- **NOT cancel-safe:** `read_exact`, `read_to_end`, `write_all`, `Mutex::lock`, `RwLock::read/write`, `Semaphore::acquire`.
- **Cancel-safe:** `mpsc::Receiver::recv`, `TcpListener::accept`, `AsyncReadExt::read` (returns partial).
- Put only cancel-safe ops directly in `select!` branches; store resumable state in a struct field, not a future-local.

```rust
tokio::select! {
    msg = rx.recv() => { /* recv is cancel-safe */ }
    _ = shutdown.cancelled() => return,
}
```

⚠ **`spawn_blocking` is for blocking I/O, not CPU work at scale.** It runs on a separate blocking pool (default max 512, `Builder::max_blocking_threads`). Past the limit calls silently queue; a running blocking task **cannot be aborted**. For CPU-bound work use `rayon` and bridge back via a `oneshot`.

```rust
async fn compress_async(data: Vec<u8>) -> Vec<u8> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    rayon::spawn(move || { let _ = tx.send(compress(&data)); });
    rx.await.expect("rayon task dropped")
}
```

**Prefer `JoinSet` over `futures::join_all` for spawned tasks.** `join_all` polls inline futures in one task and re-polls *all* pending futures on any wakeup (O(N) at large N), and doesn't surface panics cleanly. `JoinSet` spawns onto the scheduler (only the woken task re-polls), yields in completion order, aborts the rest on drop, and reports panics as `JoinError`.

```rust
let mut set = tokio::task::JoinSet::new();
for url in urls { set.spawn(fetch(url)); }
while let Some(res) = set.join_next().await {
    match res {
        Ok(Ok(data)) => process(data),
        Ok(Err(e))   => eprintln!("fetch error: {e}"),
        Err(e)       => eprintln!("task panicked: {e}"), // JoinError
    }
}
```

**Channels:** `mpsc` (bounded → backpressure; `send().await` suspends when full), `oneshot` (single value, request/response), `broadcast` (fan-out, lagging receivers drop messages), `watch` (latest-value, config reloads). Bounded `mpsc` is the default choice — unbounded hides backpressure and can OOM.

- **AFIT (`async fn` in traits), stable 1.75, is NOT dyn-compatible** — you can't form `dyn MyTrait`, and the associated future has no `Send` bound (the "Send bound problem": `tokio::spawn` consumers hit "future cannot be sent between threads"). Don't bake `+ Send` into the `async fn` (breaks single-threaded users); use `#[trait_variant::make(NameSend: Send)]` to generate both a plain and a `Send` variant.
- **Async closures** (`async || {}`) and `AsyncFn`/`AsyncFnMut`/`AsyncFnOnce` are stable since 1.85 — a true async closure can borrow across await points, replacing `Box<dyn Fn() -> Pin<Box<dyn Future>>>` bounds.
- Use **`std::pin::pin!`** (stable 1.68), not `tokio::pin!`/`pin_utils::pin_mut!` — stack pins without heap alloc (can't return the pinned value out of scope; use `Box::pin` for that).

## Smart Pointers & Interior Mutability

Pick by ownership × threading × mutability:

| Need | Single-threaded | Multi-threaded |
| --- | --- | --- |
| Shared ownership (immutable) | `Rc<T>` | `Arc<T>` |
| Interior mutability (one value) | `Cell<T>` (Copy) / `RefCell<T>` | `Mutex<T>` / `RwLock<T>` / atomics |
| Shared + mutable | `Rc<RefCell<T>>` | `Arc<Mutex<T>>` (or `Arc<RwLock<T>>` read-heavy) |
| Heap / recursive / trait object | `Box<T>` | `Box<T>` |

- `RefCell` moves borrow checking to **runtime** — `.borrow_mut()` while any borrow is live **panics**. `try_borrow_mut()` for a fallible check. `Cell` is panic-free but only for `Copy` types (get/set/replace, no references out).
- **`Rc<RefCell<T>>` graphs are usually a smell** — deep nesting, refcount churn, cycle leaks. Prefer an **arena + indices** (`Vec<Node>` with `children: Vec<usize>`): cache-friendly, no cycles, no runtime borrow panics. Break unavoidable parent↔child cycles with `Weak<T>` (`Rc::downgrade` / `.upgrade()`), else the refcount never reaches 0 and you leak.
- `Arc` clone is an atomic refcount bump — cheap, but `Arc<Mutex<T>>` under contention serializes; consider `RwLock` (read-heavy), sharding, or lock-free (`dashmap`, atomics) if it's hot.
- **Drop order matters when a field's `Drop` uses another field:** struct fields drop FORWARD (declaration order); `let` locals drop REVERSE (LIFO). Declare the field that must outlive the others **last**. A struct's own `Drop::drop()` runs before its fields.

## CLI with clap (derive)

```rust
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "loom", version, about = "Agent orchestration CLI")]
struct Cli {
    #[arg(short, long, value_name = "FILE")] config: Option<PathBuf>,
    #[arg(short, long, action = clap::ArgAction::Count)] verbose: u8, // -v -vv -vvv
    #[command(subcommand)] command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        name: String,
        #[arg(short, long, default_value = "default")] template: String,
        #[arg(long)] no_git: bool,
    },
    Status {
        #[arg(short, long, value_enum, default_value_t = OutputFormat::Table)] format: OutputFormat,
        #[arg(short, long, value_parser = validate_stage_id)] stage: Option<String>, // custom validator
    },
}

#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
enum OutputFormat { Table, Json, Yaml }

fn validate_stage_id(s: &str) -> Result<String, String> {
    if s.chars().all(|c| c.is_alphanumeric() || matches!(c, '-' | '_')) {
        Ok(s.to_string())
    } else {
        Err("stage id: alphanumeric, '-', '_' only".into()) // clap prints this + usage
    }
}
```

- Nest subcommands with a struct wrapping `#[command(subcommand)]`; dispatch with an exhaustive `match` on the enum (no `_` arm, so a new variant is a compile error).
- `default_value_t` takes the typed value; `default_value` takes a string clap re-parses. `value_parser` runs a custom fn returning `Result<T, impl Display>`.
- `#[arg(env = "LOOM_CONFIG")]` reads an env fallback; `#[arg(global = true)]` propagates a flag to subcommands.
- Return `-> anyhow::Result<()>` from `main`; clap already exits with a usage message + code 2 on parse errors.

## Cargo, Workspaces & Lints

```toml
# Cargo.toml — edition 2024 is stable since 1.85.0 (2025-02-20); use it for new code.
[package]
name = "loom"
edition = "2024"
rust-version = "1.85"      # MSRV; cargo errors if a dep needs newer

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] } # NOT "full" — see below
serde = { version = "1", features = ["derive"] }
thiserror = "2"
anyhow = "1"

[[bench]]
name = "bench"
harness = false  # required for criterion
```

```toml
# Workspace root — centralize versions AND lints (both stable since 1.74).
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]        # members inherit via `edition.workspace = true`
edition = "2024"
rust-version = "1.85"

[workspace.dependencies]   # members inherit via `serde.workspace = true`
serde = { version = "1", features = ["derive"] }

[workspace.lints.rust]
unsafe_code = "forbid"
[workspace.lints.clippy]
unwrap_used = "deny"
```

```toml
# Each member Cargo.toml opts in:
[lints]
workspace = true
```

⚠ **Feature unification is the #1 workspace surprise.** Features are **additive and unified per target across the entire build graph**: if *any* crate enables `tokio/full`, every crate that depends on tokio in that build gets `full` compiled in — you cannot rely on a feature being OFF, and one dependency can silently pull heavy features into another. `resolver = "2"` (default in edition ≥2021) separates dev/build/host-vs-target features but does **not** un-unify normal deps within one target. Consequences: depend on **narrow feature sets** (avoid blanket `"full"`); a `#[cfg(feature = "x")]` block may compile because a sibling enabled `x`. Debug with `cargo tree -e features` / `cargo tree -f "{p} {f}"`.

- `cargo add <crate>` / `cargo add <crate> -F feat1,feat2` — never hand-edit `[dependencies]` version strings.
- `cargo update -p <crate> --precise <ver>` pins one transitive dep without touching the rest.

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid() -> anyhow::Result<()> { // return Result to use `?` in tests
        let c = parse_config("name = 'x'")?;
        assert_eq!(c.name, "x");
        Ok(())
    }

    #[test]
    #[should_panic(expected = "divide by zero")]
    fn rejects_zero() { divide(1, 0); }

    #[tokio::test] // async tests
    async fn fetches() { assert!(fetch("http://x").await.is_ok()); }
}
```

- **Spread `..Default::default()` in test struct literals** so adding a field later doesn't break N call sites. Requires `#[derive(Default)]` (or a `Config::test_default()` base). This one habit saves large mechanical diffs.

```rust
let cfg = Config { name: "x".into(), ..Default::default() }; // future fields auto-filled
```

- ⚠ **`cargo clippy` lints only the default target** — `#[cfg(test)]` modules, integration tests, examples, and benches are skipped. CI must run `cargo clippy --all-targets --all-features -- -D warnings`, else clippy failures hide in test code until they surface elsewhere.
- Property tests: `proptest!` (shrinks failing cases) for round-trips/invariants over generated input. `mockall` for trait mocks; `insta` for snapshot tests.
- Integration tests live in `tests/` (each file is a separate crate, sees only the public API). Doctests in `///` run under `cargo test` — mark non-compiling examples ```` ```no_run ```` or ```` ```ignore ````.
- Loom note: many tests use `serial_test`'s `#[serial]` because they touch shared `.work/` state — they can't run in parallel.

## Patterns

**Builder** — `..Default::default()` in `new()` so only non-default fields are listed; consuming `self` methods chain:

```rust
#[derive(Default)]
pub struct RequestBuilder { url: Option<String>, timeout: Duration, method: Method }

impl RequestBuilder {
    pub fn new() -> Self { Self { timeout: Duration::from_secs(30), ..Default::default() } }
    pub fn url(mut self, url: impl Into<String>) -> Self { self.url = Some(url.into()); self }
    pub fn build(self) -> Result<Request, BuildError> {
        Ok(Request { url: self.url.ok_or(BuildError::MissingUrl)?, /* .. */ })
    }
}
```

**Newtype** — type-safe wrappers that make ID mix-ups a compile error, and the place to enforce validation once at construction:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(String);
impl UserId { pub fn as_str(&self) -> &str { &self.0 } }
// get_user(UserId) and get_order(OrderId) can no longer be transposed.
```

**`Cow<'_, B>`** — borrow on the common path, allocate only when the value must change (escaping, normalization):

```rust
use std::borrow::Cow;
fn escape(s: &str) -> Cow<'_, str> {
    if s.contains(['<', '>', '&']) {
        Cow::Owned(s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;"))
    } else {
        Cow::Borrowed(s) // zero allocation
    }
}
```

**`entry` API** — one lookup instead of contains-then-insert, and it sidesteps the "borrow map immutably then mutably" conflict:

```rust
*counts.entry(key).or_insert(0) += 1;
map.entry(key).or_insert_with(Vec::new).push(item); // or_insert_with: lazy default
```

## Anti-Patterns & Silent Footguns

**Signature hygiene** — accept `&[T]`/`&str`, not `&Vec<T>`/`&String` (callers with arrays, slices, string literals can't call the latter); iterate `&items`, don't `.clone()` to loop.

```rust
fn print_names(names: &[String], title: &str) { /* accepts Vec, array, slice */ }
```

**`?` over match-to-rethrow** — never `match … { Err(e) => return Err(e), Ok(v) => v }`; that's exactly `?`.

**No `unwrap`/`expect` in library code** — return `Result` and let the caller decide. `panic!` is only for a violated *invariant* (a bug: out-of-bounds, "impossible" branch), never for expected failures (I/O, bad input, missing resource). Encode validity in the type system (newtype constructors) so validation happens once.

⚠ **Integer `as` casts truncate/wrap silently.** `300u32 as u8 == 44`; `-1i32 as u32 == u32::MAX`; `3.9f64 as u32 == 3`; `f64::NAN as i32 == 0` (float→int saturates since 1.45). Use `u8::try_from(x)?` / `x.try_into()?` for checked narrowing on any untrusted size. `clippy::cast_possible_truncation` (pedantic) flags these.

⚠ **Plain `+ - *` panic on overflow in debug but WRAP silently in `--release`** — a reliable debug panic becomes silent corruption in production. On untrusted/user-controlled sizes use the explicit family: `checked_*` (`Option`), `saturating_*` (clamp), `wrapping_*` (always wrap), `overflowing_*` (`(value, bool)`).

⚠ **Byte-slicing a `&str` panics on a non-char-boundary.** `&s[0..n]` is a *byte* range; if `n` splits a multi-byte UTF-8 scalar it panics at runtime (`byte index N is not a char boundary`). Use `s.get(0..n)` (returns `Option`, no panic), iterate `char_indices()`/`chars()`, or index by byte offsets you got from the string itself. `.len()` is bytes, not chars.

⚠ **`let _ = expr` drops immediately — zero protection.** `let _ = mutex.lock().unwrap();` acquires and instantly releases the lock; `let _ = guard;` drops the guard now. Bind a name (`let _guard = …`) to hold to end of scope, or `drop(x)` to release explicitly. Inside `move` closures, `let _ = captured` may not even capture. (The near-identical `let _name = …` binds normally — the trap is the bare `_`.)

⚠ **`HashMap` iteration order is randomized per run** (SipHash reseeds for HashDoS resistance) — never assert on it. Tests/snapshots/serialized output that assume a stable order fail intermittently (pass locally, fail in CI). Sort a collected `Vec`, or use `BTreeMap` (key order) / `indexmap::IndexMap` (insertion order).

⚠ **`std::process::exit` skips all `Drop` impls** and doesn't flush Rust I/O buffers — lock-file/temp-file/connection cleanup is abandoned. Return from `main` via `Result` or `ExitCode` instead (runs destructors, flushes). `process::abort()` skips even C `atexit`.

```rust
use std::process::ExitCode;
fn main() -> ExitCode {
    if !setup() { return ExitCode::FAILURE; } // destructors still run
    ExitCode::SUCCESS
}
```

**String building:** `parts.join(", ")` or `String::push_str`, never `s = s + part` in a loop (reallocates each iteration). `write!(&mut buf, ...)` to build without intermediate allocations.

## Security & `unsafe`

- **`unsafe impl Send`/`Sync` is an unchecked soundness promise.** Two commonly-missed invariants: (1) if `T: Drop`, its destructor must be safe on ANY thread (why std `MutexGuard` is `!Send` on POSIX — the mutex must release on the acquiring thread); (2) raw pointers make a type `!Send + !Sync`, so wrapping them needs an explicit `unsafe impl` *plus* a proof all access is synchronized. `unsafe impl Send for SharedPtr {}` over a bare `*mut T` with no sync is UNSOUND (races).
- **Panicking across an FFI boundary is UB.** Wrap every `extern "C"` entry body in `std::panic::catch_unwind` and convert to an error code. Since 1.71+ a panic escaping `extern "C"` aborts (safe but a silent crash) — `catch_unwind` still needed to return control to C. Use `extern "C-unwind"` only when both sides support unwinding.

```rust
#[no_mangle]
pub extern "C" fn rust_process(data: *const u8, len: usize) -> i32 {
    std::panic::catch_unwind(|| {
        let slice = unsafe { std::slice::from_raw_parts(data, len) };
        process(slice)
    }).unwrap_or(-1)
}
```

- **References to `static mut` are a deny-by-default error in Rust 2024.** Use `OnceLock<T>` for lazy read-after-init globals, `Mutex`/`RwLock` for mutable shared state, `&raw mut S` / `addr_of_mut!(S)` for low-level/FFI raw access. (`SyncUnsafeCell` is still nightly.)

```rust
use std::sync::OnceLock;
static CONFIG: OnceLock<Config> = OnceLock::new();
fn config() -> &'static Config { CONFIG.get_or_init(Config::load) }
```

- `unsafe` blocks require a `// SAFETY:` comment stating the invariant the caller/code upholds (`clippy::undocumented_unsafe_blocks`). `debug_assert!` the invariant where cheap.
- `PhantomData` carries variance/ownership/auto-traits for types holding raw pointers: `PhantomData<&'a T>` covariant + `Send if T: Sync`; `PhantomData<&'a mut T>` invariant; `PhantomData<fn(T)>` contravariant. The wrong marker is subtle unsoundness — consult the Nomicon variance table.

## Modern Rust (edition 2024, since 1.85 unless noted)

- **RPIT captures all in-scope generics including lifetimes** by default. Opt out with the `use<..>` precise-capture bound (stable 1.82) when the return value doesn't borrow the input:

```rust
fn indices<T>(slice: &[T]) -> impl Iterator<Item = usize> + use<> { 0..slice.len() } // captures nothing
```

- **Let chains** (`if let … && … && let …`) are stable in edition 2024 (1.88) — flatten nested `if let`, drop intermediate `Option`/`Result` juggling. Requires `edition = "2024"`.

```rust
if let Some(user) = get_user(id)
    && user.is_active()
    && let Some(email) = user.email.as_ref()
{ send_email(email); }
```

- `gen`/`unsafe` are reserved keywords in 2024; some closure-capture and `Drop` timing changed. Run `cargo fix --edition` when migrating.

## Verification Checklists

**Before committing async code:**

- [ ] No `std::sync::MutexGuard` / `RefCell` borrow held across an `.await` (explicit `{ }` scope or `drop()` before the await)
- [ ] Every `select!` branch is cancel-safe, or resumable state lives outside the future
- [ ] Spawned futures are `Send + 'static` (owned data, `async move`); no CPU-bound work in `spawn_blocking`
- [ ] Bounded channels for backpressure; panics from tasks are observed (`JoinSet`/`JoinError`)

**Before committing any Rust:**

- [ ] `cargo clippy --all-targets --all-features -- -D warnings` clean (test/example/bench code too)
- [ ] `cargo fmt --check` clean
- [ ] No `unwrap`/`expect`/`panic!` on recoverable paths in library code
- [ ] Signatures take `&str`/`&[T]`/`impl Into<T>`, not `&String`/`&Vec<T>`
- [ ] Narrowing integer conversions use `try_into()`, not `as`; no `&str` byte-slicing on untrusted indices
- [ ] Errors: `thiserror` at library boundaries, `anyhow` + `.context()` in app code; no `anyhow` in a public library API
- [ ] `#[serde(untagged)]` variants ordered most-specific-first with round-trip tests
- [ ] No assertion on `HashMap` iteration order; deterministic output sorted or `BTreeMap`/`IndexMap`
- [ ] Every `unsafe` block has a `// SAFETY:` comment; no `unsafe impl Send/Sync` without a synchronization proof
- [ ] Feature sets are narrow (no blanket `"full"` unless needed); `cargo tree -e features` checked if a dep looks heavier than expected
