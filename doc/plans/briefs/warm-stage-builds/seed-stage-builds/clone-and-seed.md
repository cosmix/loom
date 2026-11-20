# Brief: clone-and-seed

Stage `seed-stage-builds` of `doc/plans/PLAN-warm-stage-builds.md`. Read the plan's sections "What was measured while planning", "Decisions" and "What stays cold" before writing code. The stage description's PUBLIC SURFACE block is the contract: signatures, error kinds and the meaning of every outcome are fixed there, and two other workers call these functions by those signatures while you write them.

You write the clone primitive and the seeding logic. Both run in the daemon, outside any sandbox, on the operator's machine: a path mistake here writes or reads outside the tree it should stay in.

## Files you own

- `loom/src/fs/clone_tree.rs` (new), `loom/src/fs/clone_tree_tests.rs` (new), `loom/src/fs/mod.rs` (the `pub mod clone_tree;` line)
- `loom/src/stage_cache/seed.rs` (new), `loom/src/stage_cache/seed_tables.rs` (new), `loom/src/stage_cache/seed_tests.rs` (new)
- `loom/src/stage_cache/mod.rs` (the `pub mod seed;` and `mod seed_tables;` lines)
- `loom/src/stage_cache/roots.rs` (visibility of the credential check only)

Frozen, read-only: `loom/src/stage_cache/seed_contract_tests.rs` and its `mod` line. Read the contracts first; they are the acceptance of your work.

Everything else is read-only. A change you need elsewhere goes in your report.

## Part 1: `crate::fs::clone_tree`

`clone_tree(source, destination)` and `can_clone_in(dir)`, exactly as the stage description states them.

- **macOS:** one `libc::clonefile(source, destination, CLONE_NOFOLLOW)` call on the directory. `libc` 0.2 declares `clonefile`; it has no `CLONE_NOFOLLOW` constant, so define it (value `0x0001`, from `<sys/clonefile.h>`) with a comment naming the header. `ENOTSUP` and `EXDEV` are `Unsupported`. Planning measured this call at 0.27 s for 20,179 files and 10.3 s for a 113 GB tree, against 6.7 s and 88 s for a per-file walk, so do not walk on macOS.
- **Linux:** there is no directory clone. Walk the source with descriptor-relative, no-follow opens, as `loom/src/fs/no_follow_tree.rs` does for removal (`open_directory`, `entries`, `kind`, `MAX_DEPTH` 64): create each directory, clone each regular file with `ioctl(FICLONE)` into a file created `O_CREAT | O_EXCL`, recreate each symlink from `readlinkat` with `symlinkat`, skip every other file type. Set each file's and directory's permission bits and modification time from the source (`fchmod`, `futimens`; directories after their contents). The first `EOPNOTSUPP`, `EXDEV`, `EINVAL` or `ENOTTY` from `FICLONE` means the filesystem cannot clone: remove what was created with `no_follow_tree::remove_entry` and return `Unsupported`. A tree deeper than `MAX_DEPTH` is an error, with the partial destination removed.
- **Other platforms:** `Unsupported`.
- **Never copy bytes.** No `std::fs::copy`, `copy_file_range`, `sendfile` or read/write loop anywhere in this module, on any path.
- **Checks before any write:** `symlink_metadata(source)` must be a directory (a symlink is `InvalidInput`); `symlink_metadata(destination)` must fail with `NotFound` (anything present, a dangling symlink included, is `AlreadyExists`). The destination's parent must exist; a missing parent is the ordinary `NotFound` error.
- `can_clone_in(dir)`: create a uniquely named directory holding one small file inside `dir`, clone it to a sibling name, remove both whatever the outcome, return whether the outcome was `Cloned`. Use `tempfile::Builder` in `dir` for the names.

Every `unsafe` block carries a SAFETY comment that states what makes it sound.

## Part 2: `crate::stage_cache::seed`

`seed_worktree`, `seed_host_caches`, `seed_sources`, `Seeded`, `SeedOutcome`, as the stage description states them. Put the two tables in `seed_tables.rs`.

**Worktree table.** Kind `rust` gives `target`. Kind `javascript` or `typescript` gives `node_modules`, only when the package directory holds `package.json`. Packages and kinds come from `crate::skills::project::ProjectProfile::discover(repo_root)` and `package_details()` (`loom/src/skills/project.rs:45-83`); `PackageDetail.kinds` is a `Vec<String>` and a package can have several. Skip the whole call when the profile is `truncated` only for packages it does not list; seed what it lists.

**Checks for one candidate**, in this order, each failure a `Skipped` with a reason a person can act on:

1. The source exists: `symlink_metadata` on `<repo_root>/<package>/<dir>`. A missing source yields no `Seeded` at all.
2. Every component from `repo_root` down to the source is a real directory by `symlink_metadata`, and every component from `worktree` down to the destination's parent likewise. Walk component by component; do not canonicalize, which follows symlinks and hides the problem.
3. Nothing exists at the destination.
4. `git check-ignore -q -- <relative path>` run with the worktree as its directory exits 0. Run git the way the tree's tests and helpers do, with ambient configuration shut out (`GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` pointed at a missing file, `GIT_CONFIG_NOSYSTEM=1`); find the existing helper for this before writing one.
5. `clone_tree`. `Unsupported` is `Skipped("this filesystem cannot clone")`; an error is `Skipped` with the error text.

No modification time anywhere in the worktree outside the cloned directory may change. Planning showed why: with the sources' original modification times Cargo reports the workspace crates fresh and the binary built in the first checkout runs in the second (plan, "What was measured").

**Host cache table.** Names, kinds and source resolution are in the stage description. `HOME` and `XDG_CACHE_HOME` come from `host_env` only. A relative value of any of the variables is ignored (fall through to the default). A source is used only when, after `canonicalize`, it is a directory and `exposes_credentials(path, &credential_paths(home))` is false; those two functions are private in `loom/src/stage_cache/roots.rs:182-211`, and widening them to `pub(super)` is the only edit you make in that file. For `npm`, create `<cache_root>/npm` first with a no-follow `mkdirat` on the already-open cache root (see how `StageCache::ensure_below`, `loom/src/stage_cache/mod.rs:171-196`, creates components), then clone `_cacache` into it. A destination that exists is `Skipped`.

`seed_sources` lists what would be cloned: worktree candidates that pass checks 1 and 2 on the main-checkout side, then host caches whose source passes its check. It clones nothing and creates nothing.

`LOOM_BUILD_SEED=0`: `seed_host_caches` and `seed_sources` read it from `host_env`, `seed_worktree` from the process environment. Any other value, or none, means on.

## Tests you write (`clone_tree_tests.rs`, `seed_tests.rs`)

The frozen contracts cover the refusals. Add only what they leave out:

- `clone_tree`: destination exists (`AlreadyExists`); source is a regular file (`InvalidInput`); on a filesystem that clones, a tree three levels deep comes out identical.
- `seed_worktree`: an unignored `target` in the worktree's rules is `Skipped` and nothing is created; a `javascript` package without `package.json` yields no candidate; a package with kinds `react` and `typescript` and a `package.json` seeds `node_modules` once.
- `seed_host_caches`: a project with no `python` kind does not clone `uv` even when the source exists; `LOOM_BUILD_SEED=0` in `host_env` returns an empty list and creates nothing; an existing `<cache_root>/bun` is left alone.
- Every test that needs a real clone asks `can_clone_in` first and returns when it is false. Use `tempfile` directories; put source and destination under one parent so they share a filesystem. Tests that set process environment are `#[serial_test::serial]`.

## Traps

- `clonefile` on a directory copies the tree's symlinks as symlinks. Do not "resolve" them.
- A cloned directory inherits the source's permission bits; the stage cache root is `0700` and stays so because you never touch it.
- The worktree is new and has no session yet when this runs, so nothing else is writing in it. The main checkout may be mid-build; a half-written build directory is cloned as it is and the build tool's own freshness check handles it. Do not add locking.
- Files stay at most 400 lines and functions at most 50. `loom/maintainability-baseline.txt` is a ratchet: do not raise an entry.

## Done when

From the repository root: `cargo test --manifest-path loom/Cargo.toml --lib fs::clone_tree` and `cargo test --manifest-path loom/Cargo.toml --lib stage_cache::seed` pass, the seven contracts among them, and `cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings` is clean for your files. Report: files changed, each command with its result, anything unresolved, any mistake worth recording.
