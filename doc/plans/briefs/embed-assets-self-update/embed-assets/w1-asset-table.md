# W1 — Embedded asset table (build script + module surface)

Tier: codex `gpt-5.6-terra`, effort `xhigh`.

## Files you own (write)

- `loom/build.rs` — extend it; do not rewrite what is there.
- `loom/src/assets/mod.rs` — new file.

Read-only: `loom/src/fs/permissions/constants.rs` (the existing embedding pattern),
`loom/src/version/derive.rs` (already `include!`-ed by build.rs).

## Entry points

- `main()` and `emit_rerun_keys()` in `loom/build.rs`. `emit_rerun_keys` shows the house style for
  emitting `cargo:rerun-if-changed` and for the "only emit paths that exist" rule — a path that
  does not exist is permanently dirty to cargo and forces a rebuild every time.
- `LOOM_HOOKS` in `loom/src/fs/permissions/constants.rs` — the `&[(&str, &str)]` table shape this
  work generalises. Look at it before designing anything.

`build.rs` runs with `CARGO_MANIFEST_DIR` = `<repo>/loom`, so the repository root is its parent.

**What you must preserve, byte for byte:** the `include!` of `src/version/derive.rs` (build.rs:8-11),
the four `cargo:rustc-env=LOOM_VERSION|LOOM_COMMIT|LOOM_BUILD_DATE|LOOM_TARGET` lines (`:26-29`),
`run_git`, `emit_rerun_keys` and `emit_if_exists`. `self_update/mod.rs:40` pins
`env!("LOOM_VERSION")`, so dropping one is a compile error, not a silent regression.

**The build script is `std`-only.** `loom/Cargo.toml` has no `[build-dependencies]` section and
you own no manifest, so the directory walk is a hand-rolled recursion over `std::fs::read_dir`
(`walkdir` is only a transitive dependency and is not linkable from `build.rs`).

**Three literals must appear in `build.rs` as written**, because the stage's wiring checks pin
them there: the group name `CLAUDE_AGENTS`, the generated file name `embedded_assets.rs`, and the
source root `codex/skills` (write it as the string `"codex/skills"` joined onto the repo root, not
as `.join("codex").join("skills")`). The `cargo:rerun-if-changed=` keys are proven from cargo's
record of your script's stdout at `loom/target/debug/build/loom-<hash>/output`, which must contain
lines ending in `/codex/skills` and `/AGENTS.md.template`.

## What to build

### 1. `build.rs`: generate `$OUT_DIR/embedded_assets.rs`

Add one function, called from `main()`, that walks four source roots under the repository root and
writes a Rust source file to `$OUT_DIR/embedded_assets.rs`:

| Constant | Source root | Key (first tuple field) | Selection |
| --- | --- | --- | --- |
| `CLAUDE_AGENTS` | `agents/` | file name | `*.md`, top level only |
| `CLAUDE_COMMANDS` | `commands/` | file name | `*.md`, top level only |
| `SKILLS` | `skills/` | path relative to `skills/` | every file under a directory whose name starts with `loom-`, recursive |
| `CODEX_SKILLS` | `codex/skills/` | path relative to `codex/skills/` | every file, recursive |

Plus two scalars: `CLAUDE_MD_TEMPLATE` from `CLAUDE.md.template` and `AGENTS_MD_TEMPLATE` from
`AGENTS.md.template` (both at the repository root; `AGENTS.md.template` is written by another
worker in this same stage — treat it as certain to exist by the time the stage's build runs, and
fail loudly if it does not).

Emitted shape, one row per file:

```text
pub const SKILLS: &[Asset] = &[
    ("loom-accessibility/SKILL.md", include_str!("/abs/path/to/skills/loom-accessibility/SKILL.md")),
];
```

Rules that matter:

- **Emit `include_str!` of an absolute path, never the file's bytes.** rustc then tracks each
  file's contents itself, exactly as `constants.rs` does for hooks.
- Write every path and key through `format!("{:?}", s)` so the emitted literal is escaped
  correctly. Never hand-concatenate quotes.
- **Sort rows by key** so the generated file is deterministic across machines.
- **Skip** any path component starting with `.` and any component named `__pycache__`. The
  working tree can contain an untracked `skills/loom-md-tables/__pycache__/` with `.pyc` files in
  it; embedding one is a build error, and silently skipping unknown binaries would hide a real
  problem. So: skip those two categories, and for everything that survives, `panic!` with the
  offending path if the file is not valid UTF-8.
- `skills/core-skills.txt` is a manifest, not a skill. It sits at the `skills/` root rather than
  inside a `loom-*` directory, so the selection rule already excludes it — do not special-case it,
  and do not embed it (`src/skills/index_catalog.rs` already `include_str!`s it).
- Keys always use `/` separators.
- The walk reads the working tree, not the git index: a local build embeds whatever an operator
  has under `skills/loom-*/` that survives the two skip rules. Release binaries come from CI's
  clean checkout; say this in the generator's doc comment so nobody "fixes" it into a
  `git ls-files` call that would make the build depend on git.
- Emit `cargo:rerun-if-changed=` for `../agents`, `../commands`, `../skills`, `../codex/skills`,
  `../CLAUDE.md.template`, `../AGENTS.md.template` — through the existing `emit_if_exists` helper
  so a missing path never poisons the build cache. Cargo walks a directory recursively for this
  key, so an added or deleted file re-triggers generation.

Keep the new build-script code under the repository's 50-line function limit; split into small
helpers (`walk_files`, `emit_group`, `rust_literal`) rather than writing one long function.

### 2. `loom/src/assets/mod.rs`

```rust
//! Every installable asset, embedded at build time.
//! …

/// (path relative to the group's root, file contents)
pub type Asset = (&'static str, &'static str);

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

pub mod install;

#[cfg(test)]
mod tests;
```

`pub type Asset` must be declared **before** the `include!`, since the generated file refers to it.
`pub mod install;` and `#[cfg(test)] mod tests;` name files another worker in this stage writes —
declare them anyway.

`loom/src/lib.rs` gains `pub mod assets;` from a different worker. Do not touch `lib.rs`.

### 3. Tests, inline in `mod.rs` under `#[cfg(test)] mod tests_table`

Do not create a separate test file — `assets/tests.rs` belongs to another worker.

- every group is non-empty;
- `SKILLS` contains the keys `loom-plan-writer/SKILL.md` and `loom-rust/SKILL.md`;
- `SKILLS` contains `loom-md-tables/fix-md-tables.py` — proves non-markdown skill files travel;
- no `SKILLS` key starts with `core-skills`;
- no key in any group contains `__pycache__` or a backslash;
- keys within each group are unique and sorted;
- `CODEX_SKILLS` contains `pressure/SKILL.md` and `loom-skills/SKILL.md`;
- `CLAUDE_AGENTS` contains `loom-software-engineer.md`; `CLAUDE_COMMANDS` contains `pressure.md`;
- `CLAUDE_MD_TEMPLATE` starts with `# CLAUDE.md - BINDING RULES`;
- `AGENTS_MD_TEMPLATE` is non-empty and at most **12288** bytes. Write this as a named constant
  `AGENTS_MD_TEMPLATE_MAX_BYTES: usize = 12_288` with a comment recording why: codex loads
  `~/.codex/AGENTS.md` into every session and truncates a project doc at its
  `project_doc_max_bytes` default of 32768 bytes, and loom holds itself to a tighter budget than
  the external cap. Model the assertion's failure message on
  `claude_md_template_stays_under_its_size_ceiling` in
  `loom/src/orchestrator/signals/tests_size.rs`: tell the reader to trim the regrowth rather than
  raise the ceiling.

## Done means

`cargo build --manifest-path loom/Cargo.toml` succeeds and
`cargo test --manifest-path loom/Cargo.toml --lib assets::tests_table` passes, once the other
workers' files exist. If your own slice is finished before theirs, say so in your report rather
than stubbing their files.

## Constraints the graph will not show you

- Both new files must stay under 400 lines and every function under 50 lines. The repository's
  maintainability gate is an exact ledger; a new entry is debt you must not add.
- Do not run `git` at all. Do not run the full test suite, the linter or the formatter — the
  orchestrator verifies.
