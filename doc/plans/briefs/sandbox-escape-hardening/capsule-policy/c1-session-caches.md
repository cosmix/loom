# C1: per-session caches (resolver, seeding, codex home, environment, removal)

Stage `capsule-policy`, wave 1, tier opus. Read `../common.md` first (D3 and D6 are yours to
implement at the filesystem level).

## Why

Stage sessions write the operator's real package caches today
(`sandbox/package_caches.rs` grants them), and the host builds from those caches outside every
sandbox: an agent that edits `~/.cargo/registry/src/*/<crate>/build.rs` runs code on the host at
the next host build. D3 takes every grant away and gives each session a cache directory `C` of
its own content, at a path stable per stage and session kind, seeded from the real caches
through read-only symlinks and copies. D6 moves the codex lane
onto `CODEX_HOME=C/codex-home`. You build the whole filesystem side of that as one new module;
C2 builds the capsule rules, C3 wires your functions into the launch and retirement.

## Files you own (all new)

`loom/src/sandbox/session_cache/` : `mod.rs`, `real.rs`, `ecosystems.rs`, `tree.rs`, `seed.rs`,
`codex_home.rs`, `env.rs`, `tests_real.rs`, `tests_seed.rs`, `tests_codex_home.rs`,
`tests_env.rs`.

C2 adds `pub mod session_cache;` to `loom/src/sandbox/mod.rs`; you never edit `mod.rs` of
`sandbox`. Declare your own test modules in `session_cache/mod.rs` with
`#[cfg(test)] mod tests_seed;` and so on (module paths such as
`sandbox::session_cache::tests_seed::...`; acceptance runs two of them by exact name).

## The public surface (pinned: C2, C3 and the contracts call it)

In `session_cache/mod.rs`, re-exporting from the submodules so every item is reachable as
`crate::sandbox::session_cache::<name>`:

    /// The directory under `C` that holds the codex lane's `CODEX_HOME`.
    pub const CODEX_HOME_DIR: &str = "codex-home";
    /// The owner marker in `C`: a regular file holding the id of the session the
    /// cache was prepared for. Retirement removes `C` only for that session; the
    /// capsule write-denies the marker (C2).
    pub const SESSION_OWNER_MARKER: &str = ".loom-session";
    /// Codex files copied into `C/codex-home` rather than linked, which the capsule
    /// write-denies (C2): codex reads its hook registrations and configuration from
    /// them, and a session must not be able to replace either.
    pub const CODEX_PROTECTED_FILES: [&str; 2] = ["config.toml", "hooks.json"];
    /// Every variable `session_cache_env` may set except the `GIT_CONFIG_*` entries;
    /// `process::environment` forwards exactly these to confined commands.
    pub const SESSION_CACHE_VARIABLES: [&str; 18] = [
        "CARGO_HOME", "RUSTUP_HOME", "RUSTUP_AUTO_INSTALL", "BUN_INSTALL_CACHE_DIR",
        "npm_config_cache", "npm_config_store_dir", "YARN_CACHE_FOLDER", "GOPATH",
        "GOMODCACHE", "GOCACHE", "GOFLAGS", "GOPROXY", "UV_CACHE_DIR", "UV_LINK_MODE",
        "PIP_CACHE_DIR", "DENO_DIR", "XDG_CACHE_HOME", "CODEX_HOME",
    ];
    /// The fixed git settings every session and confined command runs with, so git
    /// never tries to maintain a read-only common directory.
    pub const SESSION_GIT_CONFIG_ENV: [(&str, &str); 5] = [
        ("GIT_CONFIG_COUNT", "2"),
        ("GIT_CONFIG_KEY_0", "gc.auto"), ("GIT_CONFIG_VALUE_0", "0"),
        ("GIT_CONFIG_KEY_1", "maintenance.auto"), ("GIT_CONFIG_VALUE_1", "false"),
    ];

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct RealCaches {
        pub cargo_home: PathBuf,
        pub rustup_home: Option<PathBuf>,
        pub bun_cache: PathBuf,
        pub npm_cache: PathBuf,
        pub pnpm_store: PathBuf,
        pub yarn_cache: PathBuf,
        pub go_mod_cache: PathBuf,
        pub go_build_cache: PathBuf,
        pub go_proxy: Option<String>,
        pub uv_cache: PathBuf,
        pub pip_cache: PathBuf,
        pub deno_dir: PathBuf,
        pub xdg_cache: PathBuf,
        pub codex_home: PathBuf,
    }
    impl RealCaches {
        pub fn from_lookup(home: &Path, lookup: &dyn Fn(&str) -> Option<OsString>) -> Self;
        pub fn from_env() -> anyhow::Result<Self>;
        pub fn cache_paths(&self) -> Vec<PathBuf>;
    }

    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub struct Ecosystems { pub cargo: bool, pub bun: bool, pub npm: bool, pub pnpm: bool,
                            pub go: bool, pub uv: bool }
    impl Ecosystems { pub fn detect(dir: &Path) -> Self; }

    pub fn project_cache_key(project_root: &Path) -> String;
    pub fn project_session_cache_root(project_root: &Path) -> anyhow::Result<PathBuf>;
    pub fn session_cache_root(work_dir: &Path) -> anyhow::Result<PathBuf>;
    pub fn session_cache_key(stage_id: Option<&str>, kind: SessionType, session_id: &str)
        -> anyhow::Result<String>;
    pub fn session_cache_dir(root: &Path, key: &str) -> anyhow::Result<PathBuf>;
    pub fn prepare_session_cache(root: &Path, key: &str, session_id: &str, uid: u32,
                                 real: &RealCaches, used: Ecosystems, codex_licensed: bool)
        -> anyhow::Result<PathBuf>;
    pub fn session_cache_owner(dir: &Path) -> Option<String>;
    pub fn release_session_cache(dir: &Path, session_id: &str) -> anyhow::Result<bool>;
    pub fn seed_cargo(real_cargo_home: &Path, dest: &Path) -> anyhow::Result<()>;
    pub fn seed_codex_home(real_codex_home: &Path, dest: &Path) -> anyhow::Result<()>;
    pub fn session_cache_env(dir: &Path, real: &RealCaches, codex_licensed: bool)
        -> Vec<(String, String)>;
    pub fn remove_session_cache(dir: &Path) -> anyhow::Result<()>;

`SessionType` is `crate::models::session::SessionType`. Everything else stays `pub(super)` or
private.

**Why the path is per stage and kind while the content is per session.** Cargo's fingerprint
hashes a registry package's source path, which lies under `CARGO_HOME`. Measured under bwrap: a
build under `CARGO_HOME=c1`, then `c2`, then `c1` recompiled `cfg-if` at every change, while a
stable `CARGO_HOME` path whose content was replaced did not. So `C` is
`<root>/<stage-id>-<session-kind>`: every session of one stage and kind (a handoff, a retry)
gets the same path and no dependency rebuild. `<root>` is namespaced per project (section 7):
two clones of one host share the user cache directory, and both run a stage named
`integration-verify`, so an unnamespaced key would make the second spawn delete the first's live
cache. The content is still per session: each spawn
deletes whatever the previous session left and seeds afresh, so nothing a session wrote reaches
the next one, the real caches are never written, and the host never builds from `C` (the
operator's decision 1).

## 1. `real.rs`: where the operator's caches are

`from_lookup` is pure: it reads only `lookup` and `home`, never the process environment, so
tests and the contracts inject both. A value counts only when it is non-empty and absolute; any
other value falls back to the default. Let `xdg_cache` be `XDG_CACHE_HOME` (absolute) or
`home/.cache` on both platforms, and let `user_cache` be `xdg_cache` on Linux and
`home/Library/Caches` on macOS (`cfg!(target_os = "macos")`).

| Field | Override(s), first that counts | Default |
| --- | --- | --- |
| `cargo_home` | `CARGO_HOME` | `home/.cargo` |
| `rustup_home` | `RUSTUP_HOME` | `None` (only an explicit value is exported) |
| `bun_cache` | `BUN_INSTALL_CACHE_DIR`; else `BUN_INSTALL` + `/install/cache` | `home/.bun/install/cache` |
| `npm_cache` | `npm_config_cache`, `NPM_CONFIG_CACHE` | `home/.npm` |
| `pnpm_store` | `npm_config_store_dir`, `NPM_CONFIG_STORE_DIR`; else `PNPM_HOME` + `/store`; else Linux `XDG_DATA_HOME` + `/pnpm/store` | Linux `home/.local/share/pnpm/store`, macOS `home/Library/pnpm/store` |
| `yarn_cache` | `YARN_CACHE_FOLDER` | Linux `user_cache/yarn`, macOS `home/Library/Caches/Yarn` |
| `go_mod_cache` | `GOMODCACHE`; else first `:` entry of `GOPATH` + `/pkg/mod` | `home/go/pkg/mod` |
| `go_build_cache` | `GOCACHE` | `user_cache/go-build` |
| `go_proxy` | `GOPROXY` (any non-empty value, kept as text) | `None` |
| `uv_cache` | `UV_CACHE_DIR` | `xdg_cache/uv` (uv uses it on macOS too) |
| `pip_cache` | `PIP_CACHE_DIR` | `user_cache/pip` |
| `deno_dir` | `DENO_DIR` | `user_cache/deno` |
| `xdg_cache` | `XDG_CACHE_HOME` | `home/.cache` |
| `codex_home` | `CODEX_HOME` | `home/.codex` |

`from_env()` = `from_lookup(&dirs::home_dir().context("cannot determine the home
directory")?, &|name| std::env::var_os(name))`.

`cache_paths()` lists `cargo_home/registry`, `cargo_home/git`, `bun_cache`, `npm_cache`,
`pnpm_store`, `yarn_cache`, `go_mod_cache`, `go_build_cache`, `uv_cache`, `pip_cache`,
`deno_dir`, deduplicated. Never `cargo_home` itself (it holds `credentials.toml`), never
`codex_home`, never `xdg_cache` (the ancestor of several). Doc-comment that the contracts name
the real caches through it.

## 2. `ecosystems.rs`: which caches to seed

`Ecosystems::detect(dir)` uses `crate::skills::project::ProjectProfile::discover(dir)` (read
`loom/src/skills/project.rs`: `root`, `types: Vec<ProjectType { kind, path }>`, `packages`).
`cargo` when any type's `kind == "rust"`; `go` when any `kind == "golang"` (not `"go"`: see
`skills/project/markers.rs:8`). The lockfile flags look in `profile.root` and in
`profile.root.join(p)` for every `p` in `packages`: `bun` for `bun.lock` or `bun.lockb`, `npm`
for `package-lock.json` or `npm-shrinkwrap.json`, `pnpm` for `pnpm-lock.yaml`, `uv` for
`uv.lock` (`Path::is_file`). Nothing else is read.

`detect` ignores every package directory, in `types` and in `packages` alike, whose path
(relative to `profile.root`) has a component named `fixtures`: test data is not a project the
session builds. This repository's `loom/tests/fixtures/source/labeled/go/go.mod` would otherwise
set `go` and seed go's build cache (about 6,000 links) on every spawn.

## 3. `tree.rs`: the three filesystem primitives (never follow a symlink)

All three walk with `std::fs::read_dir` and `symlink_metadata`; a symlink found in the SOURCE
is skipped, never reproduced and never followed (a link inside a real cache could point
anywhere). A source that does not exist is `Ok(())` and creates nothing beyond `dest`.

- `pub(super) fn copy_tree(src: &Path, dest: &Path) -> Result<()>`: real directories are
  created, regular files copied with `std::fs::copy`.
- `pub(super) fn link_files(src: &Path, dest: &Path) -> Result<()>`: real directories are
  created as real directories, every regular file becomes `std::os::unix::fs::symlink(<source
  file>, <dest file>)`. This is the "replicate with symlinked files" mechanism: the tool can add
  new files beside the links, and a directory-level link would fail (npm measured EEXIST).
- `pub(super) fn link_entries(src: &Path, dest: &Path, skip: &[&str]) -> Result<()>`: one
  symlink per top-level entry of `src` (directory or regular file) whose name is not in `skip`
  and does not start with `.`.

Symlink targets are the absolute source paths as given (the real cache location), not
canonicalized. Keep each function under 50 lines; recursion depth is the tree's depth, which is
fine.

**One rule for every seed: it never reproduces a tool's control or temp entries.** That is bun's
`.tmp`, go's `trim.txt`, and any dot-entry at the top of a root `link_entries` links. A linked
control entry points at the read-only real cache, and the tool then fails where it writes to it
(sections 4 and 5 name the cases).

`remove_session_cache(dir)` lives here too (prepare and `release_session_cache` call it):

1. `symlink_metadata(dir)`: `NotFound` is `Ok`; a symlink or non-directory is removed with
   `remove_file` (never followed).
2. Otherwise restore owner access: for the directory itself and then every REAL directory
   below it (checked with `symlink_metadata().file_type().is_dir()`, never `is_dir()` on the
   path), set mode `| 0o700` with `std::fs::set_permissions` on that path, before reading it.
   Never call `set_permissions` on a symlink (it follows the link and would change a real
   cache). Go's module cache leaves `0555` directories and `remove_dir_all` cannot empty them.
3. `std::fs::remove_dir_all(dir)` (it does not follow symlinks), with context naming the path.

## 4. `seed.rs`: one function per tool

Each takes the real location and `dest` (the tool's subdirectory of `C`, already created) and
returns `Result<()>`. The sizes below are why nothing is ever copied whole.

- `seed_cargo(real, dest)` (pub): for each directory `<reg>` under `real/registry/index`,
  `copy_tree` it to `dest/registry/index/<reg>` (cargo writes `.cache/` and `config.json` there,
  so it must be a real copy; about 80 MB, 0.12 s measured). For each regular file
  `real/registry/cache/<reg>/*.crate`, symlink it to the same relative path. For each directory
  `real/registry/src/<reg>/<crate-ver>` that holds a regular file `.cargo-ok`, symlink the
  directory to the same relative path; one without `.cargo-ok` is a partial extraction and is
  skipped. Copy `config.toml` and `config` when they are regular files. Symlink `advisory-db`,
  `advisory-dbs` and `bin` when present (`cargo audit --no-fetch` reads the advisory database
  through `$CARGO_HOME`, and cargo looks for subcommands in `$CARGO_HOME/bin`; measured: `cargo
  audit --no-fetch` passes under `CARGO_HOME=C/cargo` with the index copied and `advisory-db`
  linked, the real `~/.cargo` read-only). Never create `credentials.toml`, `credentials`, `env`,
  `git/`, `.crates.toml`, `.crates2.json`, `.package-cache*` or `.global-cache`: cargo creates
  the lock and tracking files it needs, and `git/` starts empty (git dependencies re-fetch; the
  plan records it).
- `seed_bun(real, dest)`: `link_entries(real, dest, &[".tmp"])` (0.8 s for 20,045 entries
  measured), which skips `.tmp` and every other dot-entry. Bun keeps a `.tmp` directory in the
  cache root and writes into it on every install: reproduced, with `.tmp` linked to the read-only
  real cache `bun install` and `bun x markdownlint-cli2` exit 1 ("Unexpected accessing temporary
  directory"), and with a real `.tmp` both succeed. The real `~/.bun/install/cache/.tmp` exists
  on the planning host.
- `seed_npm(real, dest)`: `copy_tree(real/_cacache/index-v5, dest/_cacache/index-v5)` (116 MB,
  about 1 s measured) and `link_files(real/_cacache/content-v2, dest/_cacache/content-v2)`.
  Nothing else (`_logs`, `_npx`, `_cacache/tmp` start empty).
- `seed_pnpm(real, dest)`: for every directory `v<N>` directly in `real` (pnpm 10 uses `v10`),
  `copy_tree(real/v<N>/index, dest/v<N>/index)` and `link_files(real/v<N>/files,
  dest/v<N>/files)`. `dest` is `C/pnpm-store`: pnpm appends `v10` itself (verified with pnpm
  10.15.0: `npm_config_store_dir=<dir> pnpm store path` prints `<dir>/v10`).
- `seed_go_build(real, dest)`: `link_files(real, dest)`, then remove `dest/trim.txt` when it is
  a symlink (`symlink_metadata`; a missing one is fine), so `trim.txt` is never linked. Go
  recreates it. Reproduced with go 1.25.5 under bwrap: a read-only linked `trim.txt` holding a
  timestamp older than a day makes `go build` exit 1 ("failed to trim cache: ... read-only file
  system"). The module cache is never seeded: `GOPROXY` serves it read-only (section 6).
- `seed_uv(real, dest)`: `copy_tree` every directory directly in `real` whose name starts with
  `simple-v`, `wheels-v` or `interpreter-v`; `link_files(real/archive-v0, dest/archive-v0)`.
  Leave `sdists-*`, `git-*`, `builds-*` and `environments-*` empty (4.6 s, about 590 MB
  measured). `UV_LINK_MODE=copy` (section 6) keeps a venv from linking into `C`, which
  retirement deletes.
- `seed_xdg_cache(real, dest)`: `link_entries(real, dest, &["loom", "pnpm", "go-build", "yarn",
  "deno", "uv", "pip"])`. `XDG_CACHE_HOME` moves into `C`, and tools find read-mostly assets
  through it (Playwright browsers in `ms-playwright`, Hugging Face models, pre-commit
  environments); the links keep them reachable, still read-only. `loom` is skipped (loom's own
  state lives there), and the relocated tools' own names get fresh writable directories
  instead (pnpm needs a writable metadata cache there to write a lockfile).

## 5. `codex_home.rs`: `seed_codex_home(real, dest)` (pub)

D6. Codex writes run state into its home during every run (sqlite databases and their `-wal`
and `-shm` files, `models_cache.json`, `version.json`, `shell_snapshots/`,
`thread-writer-locks/`, `session_index.jsonl`, `history.jsonl`, `sessions/`,
`archived_sessions/`, `log/`, `.tmp/`, `tmp/`, `app-server-control/`, `app-server-daemon/`,
`ipc/`, `memories/`, `cache/`), so a link to any of those fails with EROFS. Codex also writes
inside entries of `packages`, `plugins` and `skills`: mtimes on the planning host show it writing
today in `plugins/cache/openai-curated-remote`, `plugins/.remote-plugin-install-staging`,
`packages/app-server-daemon` (`install.lock`, `auto-update-version`) and `skills/.system`, so a
per-entry link to those fails the same way. Use an allowlist, so an entry a future codex adds
starts fresh instead of failing through a link:

- copy (regular files only): `CODEX_PROTECTED_FILES` (`config.toml` keeps the
  `exclude_slash_tmp` setting loom relies on, `architecture/codex-plugin.md`), and the root
  dot-files `.codex-global-state.json`, `.personality_migration` and `.sandbox_migration` when
  present (codex rewrites them; as copies they are writable and not write-denied);
- symlink when present: `auth.json` (codex rewrites it in place with truncate+write, which
  reaches the real file C2 grants), `AGENTS.md`, `hooks`, `installation_id`,
  `loom-skill-catalog`, `rules`, `vendor_imports`;
- real directory holding `link_entries` of the real one, when present: `packages`, `plugins`,
  `skills`, with these exceptions inside them. `plugins/cache` and `skills/.system` are
  replicated as real directory trees of symlinked files (`link_files`), so codex can add files
  beside the links. `plugins/.remote-plugin-install-staging` and `packages/app-server-daemon`
  are created as fresh empty directories. Every other entry of the three directories stays a
  per-entry link (`link_entries` with `plugins/cache` and `packages/app-server-daemon` in
  `skip`; it already skips the dot-entries `.remote-plugin-install-staging` and `.system`);
- everything else, `.tmp` and `tmp` included: not created, so it starts fresh.

Name the lists as constants with a doc comment each (protected files, copied dot-files, linked
entries, the three per-entry-link directories, the two replicated trees, the two fresh
directories). The real entry names were listed from a live `~/.codex` on the planning host. Beside
the names below it holds the root dot-entries `.codex-global-state.json`,
`.personality_migration`, `.sandbox_migration` and `.tmp`; `plugins` holds `cache` and
`.remote-plugin-install-staging`, `skills` holds `.system`, and `packages` holds
`app-server-daemon`: `AGENTS.md app-server-control app-server-daemon
archived_sessions auth.json cache computer-use config.toml goals_1.sqlite hooks hooks.json
installation_id ipc log logs_2.sqlite loom-skill-catalog mcp-oauth-locks memories
memories_1.sqlite models_cache.json node_repl packages plugin-data plugins queue_1.sqlite
rollout-migrations rules session_index.jsonl sessions shell_snapshots skills sqlite
state_5.sqlite thread-writer-locks thread_history_1.sqlite tmp
tui-thread-reference-capabilities vendor_imports version.json visualizations` (plus `-wal` and
`-shm` siblings of each sqlite file). `plugin-data` is never linked: the companion's job state
lives at the real `~/.codex/plugin-data`, which C2 grants directly.

This layout is unmeasured with a live codex run: no stage's acceptance runs the codex lane. The
plan's operator step (`## After the plan (operator)`, the one-stage scratch plan with a codex
implementer) checks it after the plan merges.

## 6. `env.rs`: `session_cache_env(dir, real, codex_licensed)`

Returns, in this order (`dir` is `C`, paths rendered with `display()`):

    CARGO_HOME=C/cargo
    RUSTUP_HOME=<real.rustup_home>            only when Some
    RUSTUP_AUTO_INSTALL=0
    BUN_INSTALL_CACHE_DIR=C/bun
    npm_config_cache=C/npm
    npm_config_store_dir=C/pnpm-store
    YARN_CACHE_FOLDER=C/yarn
    GOPATH=C/go
    GOMODCACHE=C/go/pkg/mod
    GOCACHE=C/go-build
    GOFLAGS=-modcacherw
    GOPROXY=<see below>
    UV_CACHE_DIR=C/uv
    UV_LINK_MODE=copy
    PIP_CACHE_DIR=C/pip
    DENO_DIR=C/deno
    XDG_CACHE_HOME=C/xdg-cache
    the five SESSION_GIT_CONFIG_ENV entries
    CODEX_HOME=C/codex-home                  only when codex_licensed

`GOPROXY`: the fallback is `real.go_proxy` when it is set and none of its comma- or
pipe-separated entries carries userinfo (an `@` between `://` and the next `/`); otherwise
`https://proxy.golang.org,direct` (a credential in the operator's proxy URL must not reach the
session's environment). When `real.go_mod_cache/cache/download` is a directory, the value is
`file://<that dir>,<fallback>`; otherwise just the fallback. This function stats one path and
reads nothing else.

## 7. `mod.rs`: root, directory, prepare

- `project_cache_key(project_root) -> String`: the first 16 hex characters of
  `hex::encode(Sha256::digest(project_root.as_os_str().as_bytes()))` (`sha2::{Digest, Sha256}`
  and `std::os::unix::ffi::OsStrExt`; mirror `codex_lifecycle/jobs.rs:156`; `sha2` and `hex` are
  dependencies already). It hashes the path as given; its callers pass the canonical project
  root (`WorkDir::project_root`, the parent of `.loom/work`).
- `project_session_cache_root(project_root)`: the cache root of one project, the directory
  `loom init` and `loom clean` (C3) remove. Under `cfg!(test)`,
  `<project_root>/.loom/work/session-caches`, which is the directory `session_cache_root`
  returns for that project's real work dir, so a test of the cleanup finds what a test of the
  launch prepared. Otherwise canonicalize `project_root` (context naming the path), then
  `dirs::cache_dir().context(...)?.join("loom").join("session-caches").join(<project_cache_key
  of the canonical root>)` (put the join in a private `namespace_under(cache_dir, project_root)`
  so a test checks it without `dirs`), refused with an error when the result lies under `/tmp`
  (mirror `relay/scratch.rs:47`: every sandbox on the host can write `/tmp/claude-<uid>`).
- `session_cache_root(work_dir)`: under `cfg!(test)`, `<absolute work_dir>/session-caches`
  (mirror `host_scratch_root` in `orchestrator/terminal/native/launch/host.rs:201`, so no unit
  test creates directories in the operator's cache); otherwise the project root of `work_dir`
  (`crate::fs::work_dir::WorkDir::new(absolute work_dir)?.project_root()`, an error with context
  when it is `None`) passed to `project_session_cache_root`. Doc the rule and why: the stage id
  and kind alone collide across repositories that share one host cache root.
- `session_cache_key(stage_id, kind, session_id)`: with a stage,
  `validate_id(stage_id)` then `format!("{stage_id}-{kind}")` (`SessionType`'s `Display`:
  `stage`, `contract`, `knowledge`, `merge`, `base_conflict`, `adjudication`; none holds a `-`,
  so the last `-` always separates the kind); without one, `validate_id(session_id)` and the
  session id itself. Doc the rule and why (the stable path above).
- `session_cache_dir(root, key)`: refuses a key that is empty, longer than 255 bytes, or holds
  anything but ASCII letters, digits, `-` and `_` (a stage id may be 128 characters, so
  `validate_id`'s length limit does not fit the composite key), then `root.join(key)`.
- `prepare_session_cache(root, key, session_id, uid, real, used, codex_licensed)`:
  1. `crate::relay::ensure_dir_0700(root, uid)` with context "the session cache root {} is
     unusable; refusing to spawn".
  2. `dir = session_cache_dir(root, key)?`; when `symlink_metadata(&dir)` succeeds (the previous
     session of this stage and kind, a crashed spawn, or anything planted there),
     `remove_session_cache(&dir)?`. Nothing a previous session wrote survives.
  3. `ensure_dir_0700(&dir, uid)`, then write `dir/SESSION_OWNER_MARKER` holding exactly
     `session_id` (create-new, mode 0600).
  4. Create `cargo`, `bun`, `npm`, `pnpm-store`, `yarn`, `go/pkg/mod`, `go-build`, `uv`, `pip`,
     `deno`, `xdg-cache` (every exported variable names an existing directory).
  5. `seed_xdg_cache` always; each `used` ecosystem's seed; `codex-home` (created, then
     `seed_codex_home`) when `codex_licensed`. A failing seed is not fatal: log
     `tracing::warn!(session_id, tool, error = %format!("{error:#}"), ...)`, then remove and
     recreate that one subdirectory empty, so no tool ever sees a half-copied index. The tool
     then downloads what it needs.
  6. Return `dir`.
  Keep it under 50 lines by giving the seeding dispatch its own function. Seven parameters is
  clippy's limit; do not add an eighth.
- `session_cache_owner(dir)`: the marker's content (trimmed) when `dir/SESSION_OWNER_MARKER` is
  a regular file per `symlink_metadata` (never read through a symlink), else `None`.
- `release_session_cache(dir, session_id)`: `Ok(false)` when `dir` is absent or its owner is
  not `session_id` (the cache belongs to a later session of the same stage and kind, and a late
  retire of a crashed session must not delete its successor's cache); otherwise
  `remove_session_cache(dir)` and `Ok(true)`. Retirement (C3) calls this, never
  `remove_session_cache` directly.

**Where seeding runs.** `prepare_session_cache` runs synchronously, once per spawn, on the
daemon's orchestrator tick inside `start_ready_stages`
(`orchestrator/core/stage_executor.rs`), through C3's launch code. On this host one spawn that
detects cargo and bun is an 84 MB index copy plus about 30,000 symlinks, and the tick is blocked
for that long. Keep the seeds to local filesystem work: no network, no retry loop, one
`read_dir` pass per directory.

## Tests (each boundary test asserts the allowed case beside the denied one)

Build every path from a `TempDir`; get the uid from `std::fs::metadata(temp.path())?.uid()`
(`std::os::unix::fs::MetadataExt`). Fixture names come from real layouts:
`registry/index/index.crates.io-1949cf8c6b5b557f/config.json`, `.cargo-ok`,
`_cacache/index-v5`, `_cacache/content-v2`, `v10/index`, `v10/files`, `archive-v0`,
`simple-v18`.

`tests_real.rs`:

- `real_caches_follow_each_tools_override_variable`: a lookup answering every override with a
  distinct TempDir path yields each; paired with `real_caches_fall_back_to_home_defaults` (a
  lookup answering `None` yields `home/.cargo`, `home/.npm`, `home/.codex`, ... on the running
  platform's branch). A relative `CARGO_HOME` falls back to the default.
- `cache_paths_never_name_the_cargo_home_or_the_codex_home`: holds `cargo_home/registry` and
  `cargo_home/git`, not `cargo_home` nor `codex_home`.
- `detect_reads_manifests_and_lockfiles` (a tree with `Cargo.toml`, `go.mod`,
  `web/package.json` plus `web/bun.lock`, `py/pyproject.toml` plus `py/uv.lock`: cargo, go, bun
  and uv set, npm and pnpm not), paired with `detect_seeds_nothing_for_a_bare_tree` (only a
  README) and `detect_ignores_packages_under_a_fixtures_directory` (`tests/fixtures/go/go.mod`
  and `tests/fixtures/web/bun.lock` beside `web/package.json`: neither sets `go` nor `bun`; the
  same files outside `fixtures` do).

`tests_seed.rs`:

- `seed_cargo_copies_the_index_and_links_crates_and_sources` (exact name; acceptance runs it):
  index a real directory with a copied `config.json`, the crate and the `.cargo-ok` source
  directory symlinks pointing at the real paths, `config.toml` a regular file.
- `seed_cargo_skips_credentials_partial_sources_and_symlinks`: no `credentials.toml`, no
  directory without `.cargo-ok`, no reproduction of a symlink planted inside the real index.
- `seed_npm_copies_the_index_and_links_content_files`, `seed_pnpm_copies_each_store_index_and_links_files`,
  `seed_go_build_links_every_cache_file`, `seed_uv_copies_metadata_and_links_archives`,
  `seed_xdg_cache_links_all_but_loom_and_relocated_tools`,
  `seed_bun_links_every_entry_but_tmp` (`.tmp` absent beside a linked package directory).
- `seed_go_build_never_links_trim_txt` (a real `trim.txt` and a cache file in `real`: the cache
  file is a link, `dest/trim.txt` does not exist).
- `seeding_a_missing_real_cache_leaves_an_empty_directory`.
- `sandbox::session_cache::tests_seed::two_projects_get_different_cache_namespaces` (exact
  name): two TempDir project roots give different `project_cache_key` values, each exactly 16
  lowercase hex characters, and one root gives the same key twice; `namespace_under(cache, a)`
  and `namespace_under(cache, b)` differ and both lie under `cache/loom/session-caches/`.
- `preparing_one_project_spares_another_projects_cache` (exact name): `ra =
  namespace_under(cache, a)` and `rb = namespace_under(cache, b)` for two TempDir project roots;
  `prepare_session_cache(rb, "stage-1-stage", "sb", ...)` then a planted `rb/stage-1-stage/
  cargo/planted.txt`; then `prepare_session_cache(ra, "stage-1-stage", "sa", ...)`, a second
  prepare in `ra` for session `sa2`, and `release_session_cache(&ra.join("stage-1-stage"),
  "sa2")`. Afterwards `session_cache_owner(&rb.join("stage-1-stage")) == Some("sb")` and
  `planted.txt` is still there: the same stage id and kind in two repositories never address
  one cache.
- `prepare_creates_a_private_directory_and_replaces_a_leftover` (mode `0o700`; a stray file the
  previous session wrote is gone; the marker names the new session) and
  `prepare_never_follows_a_symlink_planted_at_the_session_directory` (a symlink at `root/<key>`
  pointing at a victim directory: afterwards the victim is untouched and `root/<key>` is a real
  directory).
- `session_cache_key_is_stable_per_stage_and_kind` (`stage-1` and Stage give `stage-1-stage` for
  two different session ids; Contract gives `stage-1-contract`; no stage gives the session id;
  `../x` as a stage id is refused).
- `release_removes_only_a_cache_the_session_owns` (exact name; acceptance runs it): prepared for
  `s1` then again for `s2` under one key, `release_session_cache(dir, "s1")` is `Ok(false)` and
  leaves `dir`; `release_session_cache(dir, "s2")` is `Ok(true)` and removes it; paired with
  `release_ignores_a_marker_that_is_a_symlink` (a marker replaced by a symlink to a file holding
  `s1` does not make `s1` the owner).
- `remove_restores_owner_access_and_never_follows_links` (exact name; acceptance runs it): `C`
  holds a `0555` directory with a file, and a symlink to a real `0555` directory holding a file;
  after removal `C` is gone, the real directory is still `0555` and its file still there
  (restore `0755` on it before the TempDir drops). Paired with `removing_a_missing_cache_is_ok`.
- `a_failing_seed_leaves_that_tool_empty_and_the_rest_seeded`: make one real cache unreadable
  (a `0000` directory; skip the test when running as root) and check the other tool is seeded.

`tests_codex_home.rs`:

- `seed_codex_home_copies_protected_files_links_config_and_leaves_run_state_fresh` (the contract
  scenario's layout: protected files regular copies with the real bytes, `auth.json` and `hooks`
  symlinks, `skills` a real directory of links, no `sessions`, `*.sqlite*` or
  `models_cache.json`), paired with `seeding_a_missing_codex_home_creates_nothing_inside`.
- `seed_codex_home_replicates_written_trees_and_leaves_written_directories_fresh`: with
  `plugins/cache/openai-curated-remote/x.json`, `plugins/.remote-plugin-install-staging/y`,
  `plugins/other`, `packages/app-server-daemon/install.lock`, `packages/other`,
  `skills/.system/z.md` and `skills/mine` in the real home, `plugins/cache` and `skills/.system`
  are real directories holding symlinks to the real files (a new file can be created beside
  them), `plugins/.remote-plugin-install-staging` and `packages/app-server-daemon` are real empty
  directories, `plugins/other`, `packages/other` and `skills/mine` are per-entry links, and the
  root dot-files `.codex-global-state.json`, `.personality_migration` and `.sandbox_migration`
  are regular copies with the real bytes (not write-denied; `CODEX_PROTECTED_FILES` holds
  neither) while `.tmp` and `tmp` are absent.

`tests_env.rs`:

- `session_env_points_every_relocated_variable_into_the_session_directory` (every path value
  starts with `C`; no `CODEX_HOME` without the lane) paired with
  `the_codex_lane_adds_codex_home_under_the_session_directory`.
- `goproxy_serves_the_real_module_cache_first` (a real `go_mod_cache/cache/download` directory
  gives `file://...` first) and `goproxy_drops_a_fallback_that_carries_credentials`
  (`https://u:p@proxy.example` is replaced by the default, `https://proxy.example` is kept).
- `every_emitted_name_is_a_session_cache_variable_or_git_config`.
- `rustup_home_is_exported_only_when_the_daemon_sets_it`.

## Patterns and traps

- Mirror `relay/scratch.rs` for the root rules and `ensure_dir_0700`; mirror
  `orchestrator/core/inbox_drain/entry.rs:121` (`remove_tree`) for no-follow removal. Do not
  copy `entry::remove_tree` itself: it is `pub(super)` in another territory and lacks the
  permission pass.
- `Path::is_dir()`, `is_file()` and `std::fs::metadata` follow symlinks. Inside any walk use
  `symlink_metadata`. The only place following is right is reading a real cache's top level.
- Never canonicalize a symlink target and never write through one. Every write goes to a path
  under `dest` that you created.
- No new dependency (no `walkdir`); std is enough. `anyhow::Context` on every fallible call,
  naming the path.
- Doc comments state what the code does now; the module doc explains D3 in two short
  paragraphs and says the real caches are safe to link to only because no capsule grant reaches
  them.
- `cfg!(target_os = "macos")` branches: test only the running platform's branch.

## Check

None. C2 removes items C3's files still name, so the crate compiles again only after C3 (wave
2); the main agent builds and tests then. Report the public surface exactly as you wrote it, and
any deviation from the signatures above (there should be none).
