//! The managed doctrine documents: when they are rewritten in place, when the
//! operator's own copy is preserved beside them, and which destinations the
//! installer refuses to write at all.

use std::fs;

use super::{backup_count, first_distinctive_line, paths};
use crate::assets::install::{install_doctrine, InstallPaths};
use tempfile::TempDir;

fn doctrine_paths(temp: &TempDir) -> InstallPaths {
    let paths = paths(temp);
    fs::create_dir_all(&paths.claude_dir).unwrap();
    fs::create_dir_all(&paths.codex_dir).unwrap();
    paths
}

#[test]
fn changed_managed_body_keeps_only_one_backup() {
    let temp = TempDir::new().unwrap();
    let paths = doctrine_paths(&temp);
    install_doctrine(&paths).unwrap();
    fs::write(paths.claude_dir.join("CLAUDE.md"), "first change").unwrap();

    assert_eq!(install_doctrine(&paths).unwrap().len(), 1);
    assert_eq!(backup_count(&paths.claude_dir, "CLAUDE.md"), 1);
    fs::write(paths.claude_dir.join("CLAUDE.md"), "second change").unwrap();

    assert_eq!(install_doctrine(&paths).unwrap().len(), 1);
    assert_eq!(backup_count(&paths.claude_dir, "CLAUDE.md"), 1);
}

#[test]
fn stale_managed_body_is_rewritten_without_backup() {
    let temp = TempDir::new().unwrap();
    let paths = doctrine_paths(&temp);
    install_doctrine(&paths).unwrap();
    let dest = paths.claude_dir.join("CLAUDE.md");
    fs::write(
        &dest,
        "# ───\n# claude-loom | updated 2020-01-01 00:00:00\n# ───\n\nan older template\n",
    )
    .unwrap();

    let backups = install_doctrine(&paths).unwrap();

    assert!(backups.is_empty());
    assert_eq!(backup_count(&paths.claude_dir, "CLAUDE.md"), 0);
    let content = fs::read_to_string(&dest).unwrap();
    assert!(content.contains(first_distinctive_line(crate::assets::CLAUDE_MD_TEMPLATE)));
}

/// `install.sh` stamps `installed` where the installer stamps `updated`, and
/// both mark loom's own output: neither is ever backed up.
#[test]
fn install_sh_header_is_recognised_as_managed() {
    let temp = TempDir::new().unwrap();
    let paths = doctrine_paths(&temp);
    let dest = paths.claude_dir.join("CLAUDE.md");
    let rule = "───────────────────────────────────────────────────────────";
    fs::write(
        &dest,
        format!("# {rule}\n# claude-loom | installed 2026-01-01 00:00:00\n# {rule}\n\nstale\n"),
    )
    .unwrap();

    let backups = install_doctrine(&paths).unwrap();

    assert!(backups.is_empty());
    assert_eq!(backup_count(&paths.claude_dir, "CLAUDE.md"), 0);
    let content = fs::read_to_string(&dest).unwrap();
    assert!(content.contains(first_distinctive_line(crate::assets::CLAUDE_MD_TEMPLATE)));
}

#[test]
fn symlinked_doctrine_file_is_written_through() {
    let temp = TempDir::new().unwrap();
    let paths = doctrine_paths(&temp);
    let target = temp.path().join("dotfiles/CLAUDE.md");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "operator content").unwrap();
    let link = paths.claude_dir.join("CLAUDE.md");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let backups = install_doctrine(&paths).unwrap();

    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(fs::read_to_string(&target)
        .unwrap()
        .contains(first_distinctive_line(crate::assets::CLAUDE_MD_TEMPLATE)));
    assert_eq!(backup_count(&paths.claude_dir, "CLAUDE.md"), 1);
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "operator content");
}

/// Writing through a dangling link would create the file at the link's target,
/// outside the tree loom was asked to install into.
#[test]
fn dangling_symlinked_doctrine_file_is_rejected() {
    let temp = TempDir::new().unwrap();
    let paths = doctrine_paths(&temp);
    let target = temp.path().join("dotfiles/CLAUDE.md");
    let link = paths.claude_dir.join("CLAUDE.md");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = install_doctrine(&paths).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains(&link.display().to_string()), "{message}");
    assert!(message.contains(&target.display().to_string()), "{message}");
    assert!(!target.exists());
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
}
