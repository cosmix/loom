//! The write denies a merge resolver does not get, and where they live on.
//!
//! A plan's or stage's `deny_write` guards what the stage agent may author,
//! but a merge resolver must write whatever the target branch changed: `git
//! merge` unlinks and rewrites those files, and a denied path fails it with
//! "Read-only file system". The resolver's sandbox therefore drops the
//! worktree-relative entries ([`release_worktree_write_denies`]), and the
//! daemon enforces them on the resolver's output instead, when it commits the
//! merge (`git::stage_commit`, fed by [`worktree_write_denies`]).

use std::path::Path;

use anyhow::{Context, Result};

use crate::models::stage::Stage;
use crate::plan::schema::SandboxConfig;

use super::settings::is_worktree_escape_path;
use super::{merge_config, MergedSandboxConfig};

/// `stage`'s own overrides merged over the plan's sandbox config exactly as
/// every session spawn merges them, not yet path-expanded.
fn merged(plan_sandbox: &SandboxConfig, stage: &Stage) -> MergedSandboxConfig {
    merge_config(
        plan_sandbox,
        &stage.sandbox,
        stage.stage_type,
        &stage.implementers,
    )
}

/// The stage's sandbox config, merged from the plan snapshot and the stage's
/// own overrides. A plan snapshot that cannot be read counts as one with no
/// sandbox config, as it does at every session spawn;
/// [`worktree_write_denies`] refuses to guess.
pub(crate) fn stage_sandbox(work_dir: &Path, stage: &Stage) -> MergedSandboxConfig {
    let plan_sandbox = crate::fs::work_dir::read_plan_sandbox(work_dir)
        .ok()
        .flatten()
        .unwrap_or_default();
    merged(&plan_sandbox, stage)
}

/// Whether a `deny_write` entry names a path inside the worktree: not
/// absolute, not rooted at `~` or a `$VAR`, and not a parent escape
/// (`../../**`, which never matches a worktree path).
fn names_worktree_path(entry: &str) -> bool {
    let entry = entry.trim();
    !entry.starts_with(['/', '~', '$']) && !is_worktree_escape_path(entry)
}

/// Drop the worktree-relative `deny_write` entries from `config`. Absolute,
/// `~` and `$VAR` entries stay: they guard paths outside the worktree, which
/// `git merge` never writes, so a resolver keeps those carve-outs.
pub(crate) fn release_worktree_write_denies(config: &mut MergedSandboxConfig) {
    config
        .filesystem
        .deny_write
        .retain(|entry| !names_worktree_path(entry));
}

/// The worktree-relative `deny_write` globs of `stage`'s merged sandbox
/// config: what [`release_worktree_write_denies`] takes from its resolver.
/// Unlike [`stage_sandbox`] this fails when the plan snapshot is unreadable:
/// the daemon enforces these globs on the resolver's commit, and an empty
/// answer would switch that off while the resolver runs with its denies
/// released.
pub(crate) fn worktree_write_denies(work_dir: &Path, stage: &Stage) -> Result<Vec<String>> {
    let plan_sandbox = crate::fs::work_dir::read_plan_sandbox(work_dir)
        .context("the plan's sandbox config could not be read")?
        .unwrap_or_default();
    Ok(merged(&plan_sandbox, stage)
        .filesystem
        .deny_write
        .into_iter()
        .filter(|entry| names_worktree_path(entry))
        .map(|entry| entry.trim().to_string())
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::work_dir::write_plan_sandbox;
    use crate::plan::schema::{FilesystemConfig, SandboxConfig};
    use tempfile::TempDir;

    const ENTRIES: [&str; 6] = [
        "doc/specs/**",
        ".github/workflows/**",
        "/data/shared/prod/**",
        "~/keep/**",
        "${HOME}/keep/**",
        "../../**",
    ];

    fn plan_denying(work_dir: &Path, entries: &[&str]) {
        let sandbox = SandboxConfig {
            filesystem: FilesystemConfig {
                deny_write: entries.iter().map(|entry| entry.to_string()).collect(),
                ..FilesystemConfig::default()
            },
            ..SandboxConfig::default()
        };
        write_plan_sandbox(work_dir, &sandbox).unwrap();
    }

    #[test]
    fn releasing_drops_only_worktree_relative_entries() {
        let temp = TempDir::new().unwrap();
        plan_denying(temp.path(), &ENTRIES);

        let mut config = stage_sandbox(temp.path(), &Stage::default());
        release_worktree_write_denies(&mut config);

        assert_eq!(
            config.filesystem.deny_write,
            [
                "/data/shared/prod/**",
                "~/keep/**",
                "${HOME}/keep/**",
                "../../**"
            ]
        );
    }

    #[test]
    fn the_enforced_globs_are_the_entries_released() {
        let temp = TempDir::new().unwrap();
        plan_denying(temp.path(), &ENTRIES);

        assert_eq!(
            worktree_write_denies(temp.path(), &Stage::default()).unwrap(),
            ["doc/specs/**", ".github/workflows/**"]
        );
    }

    #[test]
    fn a_plan_without_denies_enforces_nothing() {
        let temp = TempDir::new().unwrap();

        assert!(worktree_write_denies(temp.path(), &Stage::default())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn an_unreadable_plan_snapshot_fails_the_enforced_globs_but_not_the_spawn_config() {
        let temp = TempDir::new().unwrap();
        // Valid TOML whose `deny_write` is not a list: the section cannot be read.
        std::fs::write(
            temp.path().join("config.toml"),
            "[plan_sandbox.filesystem]\ndeny_write = \"doc/specs/**\"\n",
        )
        .unwrap();

        let error = worktree_write_denies(temp.path(), &Stage::default()).unwrap_err();

        assert!(
            format!("{error:#}").contains("the plan's sandbox config could not be read"),
            "{error:#}"
        );
        let no_config = TempDir::new().unwrap();
        assert_eq!(
            stage_sandbox(temp.path(), &Stage::default())
                .filesystem
                .deny_write,
            stage_sandbox(no_config.path(), &Stage::default())
                .filesystem
                .deny_write
        );
    }
}
