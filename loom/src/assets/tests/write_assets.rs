//! Backup coverage for agent and command assets, the counterpart to
//! `doctrine.rs` for `claude::write_assets`. Three shipped command names
//! (`address`, `pressure`, `distill`) are not loom-namespaced, so an
//! operator's own file under one of those names must be preserved rather
//! than overwritten in place.

use std::fs;

use super::{asset_content, backup_count, paths, snapshot, the_backup};
use crate::assets::install::install_all;
use crate::skills::SkillLayout;
use tempfile::TempDir;

#[test]
fn changed_command_asset_is_backed_up_once_then_left_alone() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    install_all(&paths, Some(SkillLayout::Core), false).unwrap();
    let dest = paths.claude_dir.join("commands/address.md");
    fs::write(&dest, "operator's own /address command").unwrap();

    let report = install_all(&paths, Some(SkillLayout::Core), false).unwrap();

    assert_eq!(report.backups.len(), 1);
    assert_eq!(
        backup_count(&paths.claude_dir.join("commands"), "address.md"),
        1
    );
    assert_eq!(
        fs::read_to_string(&report.backups[0]).unwrap(),
        "operator's own /address command"
    );
    let shipped = asset_content(crate::assets::CLAUDE_COMMANDS, "address.md");
    assert_eq!(fs::read_to_string(&dest).unwrap(), shipped);

    // Reinstalling again over the now-shipped content must not add a second
    // backup or touch the tree.
    let before = snapshot(temp.path());
    let report = install_all(&paths, Some(SkillLayout::Core), false).unwrap();
    assert!(report.backups.is_empty());
    assert_eq!(snapshot(temp.path()), before);
}

/// A later loom release that ships an edited `address.md` must not rotate the
/// existing backup out from under the operator: `back_up_existing`'s rotation
/// (right for the banner-carrying doctrine files) would otherwise delete the
/// operator's own preserved original the moment loom's own output changes
/// underneath it. `back_up_existing_once` is the fix under test here - it
/// must see a backup already exists and leave it alone.
#[test]
fn a_second_diverging_install_does_not_rotate_the_operators_backup() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    install_all(&paths, Some(SkillLayout::Core), false).unwrap();
    let dest = paths.claude_dir.join("commands/address.md");
    fs::write(&dest, "operator's own /address command").unwrap();

    // First divergence: backs up the operator's file, writes loom's content.
    install_all(&paths, Some(SkillLayout::Core), false).unwrap();
    assert_eq!(
        backup_count(&paths.claude_dir.join("commands"), "address.md"),
        1
    );

    // Simulate the next loom release shipping a different `address.md`: the
    // destination must differ from the ASSET on the next install, not merely
    // from what a naive test would write, so overwrite `dest` with a third,
    // distinct string rather than reusing either prior value.
    fs::write(&dest, "loom's own next-release content, not yet installed").unwrap();
    let report = install_all(&paths, Some(SkillLayout::Core), false).unwrap();

    assert!(report.backups.is_empty());
    let commands_dir = paths.claude_dir.join("commands");
    assert_eq!(
        backup_count(&commands_dir, "address.md"),
        1,
        "a second backup must not have been created"
    );
    assert_eq!(
        fs::read_to_string(the_backup(&commands_dir, "address.md")).unwrap(),
        "operator's own /address command",
        "the backup must still hold the operator's original text"
    );
}

/// Writing through a dangling link would create the file at the link's
/// target, outside the tree loom was asked to install into. Mirrors
/// `doctrine::dangling_symlinked_doctrine_file_is_rejected` for
/// `write_assets`.
#[test]
fn dangling_symlinked_command_asset_is_rejected() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let target = temp.path().join("dotfiles/address.md");
    fs::create_dir_all(paths.claude_dir.join("commands")).unwrap();
    let link = paths.claude_dir.join("commands/address.md");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let error = install_all(&paths, Some(SkillLayout::Core), false).unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains(&link.display().to_string()), "{message}");
    assert!(message.contains(&target.display().to_string()), "{message}");
    assert!(!target.exists());
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
}

/// A live symlinked destination is written through rather than replaced:
/// `~/.claude/commands/` is sometimes itself a symlink into a dotfiles
/// repository, the same deliberate policy `write_managed_file` applies to
/// the doctrine files. The link itself must survive, holding loom's content
/// at its target.
#[test]
fn live_symlinked_command_asset_is_written_through() {
    let temp = TempDir::new().unwrap();
    let paths = paths(&temp);
    let target = temp.path().join("dotfiles/address.md");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, "operator's own /address command").unwrap();
    fs::create_dir_all(paths.claude_dir.join("commands")).unwrap();
    let link = paths.claude_dir.join("commands/address.md");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    install_all(&paths, Some(SkillLayout::Core), false).unwrap();

    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    let shipped = asset_content(crate::assets::CLAUDE_COMMANDS, "address.md");
    assert_eq!(fs::read_to_string(&target).unwrap(), shipped);
}
