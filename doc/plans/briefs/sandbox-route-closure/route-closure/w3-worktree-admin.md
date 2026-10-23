# W3: deny the sibling worktrees' administrative directories

Tier: opus (`loom-senior-software-engineer`, effort high): security-sensitive capsule policy. Read `doc/plans/token-optim/briefs/sandbox-route-closure/common.md` first.

## Goal

A session must not write another stage's git administrative directory (`<common dir>/worktrees/<name>`: its `HEAD`, `index`, `ORIG_HEAD`, `MERGE_HEAD`, `logs/`, `gitdir`, `config.worktree`). Today Claude Code grants a linked-worktree session the whole common directory and loom denies only each `config.worktree` (`session_denies.rs`, the `worktree_admin_dirs` loop, about lines 86-90). After this change:

- A worktree-rooted session (Stage, Contract, Merge) keeps its own administrative directory writable, with only that directory's `config.worktree` denied as now; every other administrative directory present at spawn is denied whole, in both layers.
- A checkout-rooted session (Knowledge, BaseConflict, and an Adjudication judge whose stage worktree is gone) has no own directory: every administrative directory present at spawn is denied whole. A judge runs in the disputed stage's worktree while `.worktrees/<id>` is a directory (`orchestrator/adjudication/session.rs` `judge_cwd`) and is worktree-rooted then, like Stage, Contract and Merge: whether a capsule is worktree-rooted follows its cwd (`sandbox/settings.rs` `target_is_worktree`), never its kind.
- `refs/`, the object store and the common directory itself stay writable: `git add` writes objects and the session's own `index`, and the merge resolver runs `git merge --no-commit --no-ff` in its own worktree (`orchestrator/signals/merge.rs` `format_merge_task`). This is what keeps the shipped worktree resolver working.
- No administrative-directory `denyWrite` entry is emitted beneath another `denyWrite` directory, and none has one beneath it: a sibling directory denied whole gets no `config.worktree` entry inside it. bubblewrap refuses to create a mount point inside a read-only bind (common.md), and a sibling's directory vanishes when that stage merges while this session still runs.

## Files you own

- `src/git/worktree/pinned.rs` (visibility of one function only)
- `src/git/worktree/config_worktree.rs`
- `src/git/worktree/config_worktree_tests.rs`
- `src/git/worktree/mod.rs` (one `pub use` line)
- `src/sandbox/control_surfaces/session_denies.rs`
- `src/sandbox/control_surfaces/tests_session_denies.rs` (remove one test, two visibility edits)
- `src/sandbox/control_surfaces/tests_admin_dir_denies.rs` (new)
- `src/orchestrator/terminal/native/tests_capsule_denies.rs`
- `src/orchestrator/terminal/native/tests_confinement_srt.rs` (`srt_settings` only)

The main agent has already appended three lines at the end of `tests_capsule_denies.rs` that declare the child module `bwrap` (`#[path = "tests_capsule_bwrap.rs"]`); leave them in place. W5 writes that file in a later wave and calls this file's `register_worktrees` and `kinds_with_cwd` from the child module, so keep both names and signatures.

## Steps

1. **`pinned.rs`.** Make `names_work_tree` `pub(super)`. Nothing else changes there.
2. **`config_worktree.rs`.** Add, below `worktree_admin_dirs`:

   ```rust
   /// The administrative directories of `repo_root`'s worktrees, split by the
   /// session that runs at `worktree`.
   #[derive(Debug, Clone, Default, PartialEq, Eq)]
   pub struct AdminDirs {
       /// The directory whose `gitdir` records `<worktree>/.git`; `None` for a
       /// session in the checkout.
       pub own: Option<PathBuf>,
       /// Every other administrative directory present now.
       pub siblings: Vec<PathBuf>,
   }

   pub fn partition_admin_dirs(repo_root: &Path, worktree: Option<&Path>) -> Result<AdminDirs>
   ```

   Behaviour, in this order: list with `worktree_admin_dirs(repo_root)?`; an empty list returns `AdminDirs::default()` without touching `worktree` (fixtures pass worktree paths that do not exist); `worktree == None` returns every listed directory as a sibling; otherwise canonicalize `worktree` (error with context naming it on failure) and take as `own` the directories for which `super::pinned::names_work_tree(dir, &canonical)` holds. Zero matches, or more than one, is an error naming the worktree and `<common dir>/worktrees`, mirroring `pinned.rs` `registered_admin_dir`: a capsule built for an unregistered worktree, or a registry with two entries claiming one worktree, refuses the spawn. Doc comment: the listing is taken at spawn, so a worktree registered later is covered only for its `config.worktree` content (`WorktreeGit::run`'s `check_worktree_config`), and its `HEAD`, `index`, `gitdir` and `logs/` stay writable to a session spawned before it. A directory on disk that no registry entry claims refuses every worktree-rooted spawn there, the adjudication judge (`judge_cwd`) and the merge resolver (`merge_resolver_worktree` in `orchestrator/terminal/backend.rs`) included, since both check only `is_dir()`: fail closed.
3. **`mod.rs`.** Change `pub use config_worktree::worktree_admin_dirs;` to `pub use config_worktree::{partition_admin_dirs, worktree_admin_dirs, AdminDirs};`.
4. **`session_denies.rs`.** Replace the comment and loop

   ```rust
   // With `extensions.worktreeConfig` git reads each worktree's
   // `config.worktree`, which could name a command the daemon's git runs.
   for admin in worktree_admin_dirs(repo)? {
       denies.absolute(&admin.join("config.worktree"), false)?;
   }
   ```

   with `deny_worktree_admin_dirs(&mut denies, repo, inputs.worktree)?;` and add:

   ```rust
   fn deny_worktree_admin_dirs(denies: &mut SessionDenies, repo: &Path, worktree: Option<&Path>) -> Result<()>
   ```

   It calls `partition_admin_dirs`; for `own`, `denies.absolute(&own.join("config.worktree"), false)?`; for each sibling, `denies.absolute(sibling, true)?`. Its doc comment states the four bullets of the goal in short sentences: why the own `config.worktree` stays denied (with `extensions.worktreeConfig` git reads it, and it could name a command the daemon's pinned git runs), why siblings are denied whole, why nothing nests, what stays writable. Update the import, and rewrite `session_denies`'s own doc comment, whose "the `config.worktree` of every worktree that exists now" is no longer the whole truth. `session_denies` shrinks; keep every other line as it is.
5. **`config_worktree_tests.rs`.** Tests for `partition_admin_dirs` on real `git worktree add` fixtures (`contract_worktree` plus a second `git worktree add -q -b loom/s2 .worktrees/s2`, as `tests_session_denies.rs::repo_with_two_worktrees` builds it). Build expected paths from `repo.join(".git").canonicalize()`, as `every_admin_directory_is_listed_and_symlinks_are_not` does: `repository()` returns the non-canonical `tmp.path().join("repo")`. Tests:
   - `the_session_worktree_is_its_own_and_every_other_is_a_sibling`: `Some(s1)` gives `own == <common>/worktrees/s1` and `siblings == [<common>/worktrees/s2]`; `None` gives no own and both as siblings.
   - `the_own_directory_is_found_through_a_symlinked_path`: pass `s1` through a symlink to the repository's parent; `own` is still found.
   - `an_unregistered_worktree_beside_registered_ones_is_refused`: `Some(<repo>/.worktrees/ghost)` created with `create_dir_all` is an error.
   - `two_directories_claiming_one_worktree_are_refused`: copy `s1`'s `gitdir` file content over `s2`'s; `Some(s1)` is an error.
   - `a_repository_without_worktrees_partitions_to_nothing`: a directory with no `.git` gives `AdminDirs::default()` for `Some` of a path that does not exist.
6. **Session-level tests, in a new file.** `tests_session_denies.rs` is 320 lines and these tests would bring it near 400, so they go in `src/sandbox/control_surfaces/tests_admin_dir_denies.rs`, a second test child of `session_denies`: declare it in `session_denies.rs` right after the existing `#[path = "tests_session_denies.rs"] mod tests;` as `#[cfg(test)]`, `#[path = "tests_admin_dir_denies.rs"]`, `mod tests_admin_dir_denies;`. In `tests_session_denies.rs`, delete `every_worktrees_config_worktree_is_denied_to_every_session_kind` (its replacement is the first test below) and make `repo_with_two_worktrees` and `has` `pub(super)` in place, so the new file calls them as `super::tests::repo_with_two_worktrees()` and `super::tests::has(..)`. Module doc of the new file: the administrative directory denies of a session, by its relation to each directory. Tests, each over `Some(<repo>/.worktrees/s1)` and `None` with `repo_with_two_worktrees` and the `DenyInputs` literal the deleted test used:
   - `each_admin_dir_is_denied_by_its_relation_to_the_session`: for `Some(s1)`: `<common>/worktrees/s1/config.worktree` in `deny_write` and as `Edit(/<path>)`; `<common>/worktrees/s1` itself in neither layer; `<common>/worktrees/s2` in `deny_write` and as `Edit(/<path>/**)`; no entry under `<common>/worktrees/s2/`. For `None`: both directories denied whole and no `config.worktree` entry at all.
   - `no_admin_dir_deny_lies_beneath_another_deny`: for both cases, no `deny_write` entry is a strict descendant of another `deny_write` entry that names an administrative directory.
   - `the_refs_the_objects_and_the_common_dir_stay_writable`: for both cases, no `deny_write` entry, taken as a `Path` `e`, satisfies `common.starts_with(e)`, `common.join("refs").starts_with(e)` or `common.join("objects").starts_with(e)`: none is the common directory, `refs` or `objects`, or an ancestor of one. Compare paths, never strings: `<common>/hooks` and `<common>/config` are denied, and their strings contain `<common>`.
   - `an_unregistered_worktree_refuses_the_capsule_denies`: `session_denies` with `Some(<repo>/.worktrees/ghost)` (created with `create_dir_all`) beside the two registered worktrees is an error.
7. **`tests_capsule_denies.rs`.** Rewrite `every_session_kind_denies_every_worktrees_config_worktree` the same way at capsule level, as `every_session_kind_denies_the_admin_dirs_it_does_not_own`, over `kinds_with_cwd`: Stage, Contract and Merge in `s1` deny `s2` whole and only `s1/config.worktree` of their own; the checkout kinds deny `s1` and `s2` whole. Assert both layers (`DENY_WRITE` and `DENY_RULES`). The Merge row is the proof that the resolver keeps its own directory. Update the module doc's first line, which names the `config.worktree` denies. Also cover an Adjudication judge whose stage worktree exists: an Adjudication capsule with cwd `s1` denies `s2` whole and only `s1/config.worktree` of its own. Add that row inside this test; never add it to `kinds_with_cwd`, which W5's probe and other tests read. Rewrite `kinds_with_cwd`'s doc comment (about lines 15-16), which says Adjudication runs in the checkout: its Adjudication row models the fallback when the stage worktree is gone.
8. **`tests_confinement_srt.rs`, `srt_settings`.** It builds `deny_write` from the capsule's `denyWrite` and then appends `claude_code_git_denies(&common)`, which lists every `worktrees/*/commondir` whether or not it exists; the e2e fixture's admin directories have none. With a sibling directory now denied whole, that order binds an absent file beneath a read-only directory, which bubblewrap refuses (common.md). Claude Code pushes its built-in denies first (measured on 2.1.302, re-read on 2.1.306). Reorder the function so `deny_write` starts with the emulated git denies (inside the existing `if let Some(common)`) and the capsule's `list("denyWrite")` entries follow, with a one-line comment naming Claude Code's order. Change nothing else in the file; never edit `tests_confinement_e2e.rs` (398 lines). These tests skip without `srt` and `socat`, so no check here proves the reorder: say so in your report.

## Traps

- `partition_admin_dirs` must not canonicalize a worktree when no administrative directory exists: many fixtures pass `/repo/.worktrees/s1`, which does not exist.
- `worktree_admin_dirs` already skips symlinked registry entries; keep relying on it.
- Two existing tests change on purpose (the one step 6 deletes and the one step 7 rewrites). The stage files one `loom stage dispute-integrity` for those `TI-edit` events after its final review round; you only write the new tests.
- Fixtures outside your files build registries too: `tests_confinement_e2e.rs` `repo_with_worktree` writes two admin directories, `gitdir` only in its own one (`tests_confinement_srt.rs` `link_worktree`), so `partition_admin_dirs` finds one own directory there and the other as a sibling. `tests_capsule.rs` and `tests_launch_capsule.rs` fixtures have no `.git`, so they partition to nothing. Keep both true.
- Never deny `<common>`, `<common>/refs` or `<common>/objects`, and never add a ref deny: the metaplan leaves ref denies out of scope.

## Your one check

`cargo test --lib -- git::worktree:: sandbox::control_surfaces::` from `loom/`, after `cargo fmt` (libtest takes both filters). The capsule-level tests run in the main agent's check after every worker returns. Report the summary line and `wc -l` of every file you changed.
