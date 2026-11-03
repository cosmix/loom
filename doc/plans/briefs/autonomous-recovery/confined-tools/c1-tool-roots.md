# confined-tools C1: which host paths a confined command's tools need (codex unit)

Lane: codex terra (`--model gpt-6.1-terra --effort xhigh`), foreground, Bash timeout 600000 ms.
Wave 1, alone. Read `doc/plans/briefs/autonomous-recovery/common.md` first. Do not run git or
touch `.loom/`. Compile-only check: `cargo build --lib`.

## Files

Own: `src/verify/criteria/tool_roots.rs` (the orchestrator created it with one `//!` line and
declared it in `src/verify/criteria/mod.rs`) and `src/verify/criteria/tool_roots_tests.rs`.
Never edit `mod.rs` or `tool_diagnosis.rs` (W2's). Read: `src/verify/criteria/confine_host.rs` (`read_roots`), `src/fs/permissions/state_root.rs`
(`CREDENTIAL_DENY_READ_PATHS`, `CREDENTIAL_FILE_DENY_READ_PATHS`),
`src/sandbox/settings/policy.rs` (`mandatory_deny_read`).

## Why

A confined criterion (bubblewrap on Linux, Seatbelt on macOS) reads only fixed roots
(`confine_host.rs::read_roots`): system directories, `~/.rustup`, `~/.bun/bin`,
`~/.bun/install/cache`, the uv cache and Python, Cargo's `bin`/`registry`/`git`. A tool on
`PATH` whose link points elsewhere is a dangling link inside the boundary, and `sh` exits 127.
Plan 06's `before_stage` check for `srt` failed that way: `~/.bun/bin/srt` links into
`~/.bun/install/global/node_modules`.

## Interface (pinned; the contracts call it)

```rust
/// The first word of every simple command in `script`, in order, without repeats, at most
/// `MAX_TOOL_WORDS` (16). Commands split at `&&`, `||`, `;`, `|`, `&`, newlines, `(`, `)`,
/// `$(` and backticks. Leading `NAME=value` assignments are skipped, and so are the words
/// `!`, `exec`, `command`, `env` (with `env`'s own assignments and `-`-options), `time`,
/// `nice`. A word holding `$`, a backtick, `*`, `?`, `[`, `=` or a quote is not a tool word.
pub(crate) fn tool_words(script: &str) -> Vec<String>;

/// Read-only roots that let `program` (an absolute path as `PATH` resolution found it on
/// the host) run inside the boundary, canonical, without repeats.
pub(crate) fn tool_roots(program: &Path, path_var: Option<&OsStr>, home: Option<&Path>) -> Vec<PathBuf>;

pub(crate) const MAX_TOOL_WORDS: usize = 16;
```

## Rules for `tool_roots` (apply in order; each hop of a symlink chain counts)

1. The directory holding `program`, when no existing root (the fixed list in `read_roots`)
   contains it: `~/.local/bin`, an nvm `<prefix>/bin`, a Linuxbrew `bin`.
2. Follow the symlink chain from `program` (at most 8 hops, relative links resolved against the
   link's directory) to its canonical target `T`. Expose the install root of `T`:
   - the nearest ancestor holding `pyvenv.cfg` (uv tool: `~/.local/share/uv/tools/<tool>`;
     pipx: `~/.local/share/pipx/venvs/<pkg>`);
   - else the OUTERMOST ancestor named `node_modules` (Bun global:
     `~/.bun/install/global/node_modules`; npm and nvm: `<prefix>/lib/node_modules`), because
     a global package's dependencies are its siblings there;
   - else the `<...>/Cellar/<formula>/<version>` ancestor (Homebrew keg);
   - else `T` itself (a single file).
3. When `T` starts with `#!`, its interpreter: `#!/usr/bin/env <name>` resolves `<name>` on
   `path_var`; `#!/abs/path` is taken as is; apply rules 1-3 to it, at most 3 interpreters deep.
4. Safety: drop a root that is `/`, the home directory, an ancestor of the home directory, or
   that contains or lies inside a credential path (the two `state_root.rs` lists and
   `mandatory_deny_read(false)`, each `~` expanded against `home`). Never return a root for a
   word that does not resolve.

Captured layouts (from a real host; build the test fixtures from these, in a `TempDir` playing
`HOME`):

```text
~/.bun/bin/prettier -> ../install/global/node_modules/prettier/bin/prettier.cjs   (#!/usr/bin/env node)
~/.local/bin/comfy  -> ~/.local/share/uv/tools/comfy-cli/bin/comfy                 (#!<tool>/bin/python)
~/.local/share/uv/tools/comfy-cli/pyvenv.cfg
~/.nvm/versions/node/v22.18.0/bin/npm -> ../lib/node_modules/npm/bin/npm-cli.js    (node beside it in bin/)
/usr/local/bin/pnpm -> ../lib/node_modules/pnpm/bin/pnpm.cjs
```

## Steps

1. Write `tool_words` and `tool_roots` with the rules above, each function under 50 lines
   (helpers allowed), the file under 300 lines.
2. Write `tool_roots_tests.rs` (declared `#[cfg(test)] #[path = "tool_roots_tests.rs"] mod
   tests;` at the bottom of `tool_roots.rs`): one test per rule and per captured layout,
   symlinks made with `std::os::unix::fs::symlink`.
3. `cargo build --lib`.
