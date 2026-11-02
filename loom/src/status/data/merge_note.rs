//! What a stage's merge left for the operator: the typed block that holds it
//! and changes the merge stashed but could not restore.

use crate::fs::work_dir::WorkDir;
use crate::git::runner::run_git;
use crate::models::stage::{Stage, StageStatus};

/// Prefix of every backup ref a merge keeps its stashed changes under.
const AUTOSTASH_REF_PREFIX: &str = "refs/loom/autostash/";

/// The typed block's sentence, while the stage is `MergeBlocked` and carries one.
pub(super) fn merge_block_text(stage: &Stage) -> Option<String> {
    if stage.status != StageStatus::MergeBlocked {
        return None;
    }
    stage.merge.block.as_ref().map(ToString::to_string)
}

/// The backup refs of changes a merge stashed and could not put back, joined
/// by ", ", for every one that still exists in the repository: deleting a ref
/// is how the operator closes its warning. One git call per recorded ref, and
/// only for a stage with unrestored changes.
pub(super) fn stash_warning(stage: &Stage, work_dir: &WorkDir) -> Option<String> {
    if stage.merge.unrestored.is_empty() {
        return None;
    }
    let repo_root = work_dir.project_root()?;
    let existing: Vec<&str> = stage
        .merge
        .unrestored
        .iter()
        .map(String::as_str)
        // The refs come from a stage file: refuse anything git could read as an option.
        .filter(|backup_ref| backup_ref.starts_with(AUTOSTASH_REF_PREFIX))
        .filter(|backup_ref| {
            let args = ["rev-parse", "--verify", "--quiet", backup_ref];
            run_git(&args, repo_root).is_ok_and(|output| output.status.success())
        })
        .collect();
    (!existing.is_empty()).then(|| existing.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::runner::run_git_checked;
    use crate::git::{MergeBlock, StashReapply};
    use crate::models::stage::MergeRecord;
    use std::path::Path;

    const BACKUP: &str = "refs/loom/autostash/s-1";

    /// A repository with one commit and the stage state directory in it.
    fn repo_work_dir() -> (tempfile::TempDir, WorkDir) {
        let tmp = tempfile::TempDir::new().unwrap();
        git(tmp.path(), &["init", "-q", "-b", "main"]);
        git(
            tmp.path(),
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "init",
            ],
        );
        let work_dir = WorkDir::new(tmp.path()).unwrap();
        work_dir.initialize().unwrap();
        (tmp, work_dir)
    }

    fn git(root: &Path, args: &[&str]) {
        run_git_checked(args, root).unwrap();
    }

    fn stage_with_stash(restored: bool) -> Stage {
        stage_with_stashes(&[(BACKUP, restored)])
    }

    /// A stage that recorded each `(backup ref, restored)` note in order.
    fn stage_with_stashes(notes: &[(&str, bool)]) -> Stage {
        let mut stage = Stage {
            id: "s".to_string(),
            ..Stage::default()
        };
        for (backup_ref, restored) in notes {
            stage.record_merge_stash(StashReapply {
                backup_ref: backup_ref.to_string(),
                restored: *restored,
            });
        }
        stage
    }

    #[test]
    fn the_stash_warning_lasts_while_the_backup_ref_exists() {
        let (tmp, work_dir) = repo_work_dir();
        let stage = stage_with_stash(false);
        assert_eq!(stash_warning(&stage, &work_dir), None);

        git(tmp.path(), &["update-ref", BACKUP, "HEAD"]);
        assert_eq!(stash_warning(&stage, &work_dir), Some(BACKUP.to_string()));

        git(tmp.path(), &["update-ref", "-d", BACKUP]);
        assert_eq!(stash_warning(&stage, &work_dir), None);
    }

    #[test]
    fn a_restored_stash_or_a_foreign_ref_gives_no_warning() {
        let (tmp, work_dir) = repo_work_dir();
        git(tmp.path(), &["update-ref", BACKUP, "HEAD"]);
        assert_eq!(stash_warning(&stage_with_stash(true), &work_dir), None);

        let foreign = stage_with_stashes(&[("HEAD", false)]);
        assert_eq!(stash_warning(&foreign, &work_dir), None);
        assert_eq!(stash_warning(&Stage::default(), &work_dir), None);
    }

    #[test]
    fn every_unrestored_backup_that_still_exists_is_warned() {
        let (tmp, work_dir) = repo_work_dir();
        let second = "refs/loom/autostash/s-2";
        let stage = stage_with_stashes(&[(BACKUP, false), (second, false)]);
        git(tmp.path(), &["update-ref", BACKUP, "HEAD"]);
        git(tmp.path(), &["update-ref", second, "HEAD"]);
        assert_eq!(
            stash_warning(&stage, &work_dir),
            Some(format!("{BACKUP}, {second}"))
        );

        git(tmp.path(), &["update-ref", "-d", BACKUP]);
        assert_eq!(stash_warning(&stage, &work_dir), Some(second.to_string()));
    }

    #[test]
    fn a_restored_note_does_not_join_the_warning() {
        let (tmp, work_dir) = repo_work_dir();
        let restored = "refs/loom/autostash/s-2";
        let stage = stage_with_stashes(&[(BACKUP, false), (restored, true)]);
        git(tmp.path(), &["update-ref", BACKUP, "HEAD"]);
        git(tmp.path(), &["update-ref", restored, "HEAD"]);
        assert_eq!(stash_warning(&stage, &work_dir), Some(BACKUP.to_string()));
    }

    #[test]
    fn the_block_text_shows_only_while_the_stage_is_merge_blocked() {
        let mut stage = Stage {
            status: StageStatus::MergeBlocked,
            merge: MergeRecord {
                block: Some(MergeBlock::TargetMoved),
                ..MergeRecord::default()
            },
            ..Stage::default()
        };
        assert_eq!(
            merge_block_text(&stage),
            Some(MergeBlock::TargetMoved.to_string())
        );
        stage.status = StageStatus::Completed;
        assert_eq!(merge_block_text(&stage), None);
        stage.status = StageStatus::MergeBlocked;
        stage.merge.block = None;
        assert_eq!(merge_block_text(&stage), None);
    }
}
