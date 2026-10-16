use std::fs;

use anyhow::Result;

use super::*;

#[test]
fn explicit_root_covers_live_and_archived_sessions() -> Result<()> {
    let root = tempfile::tempdir()?;
    let live = root.path().join("sessions/2026/09/12");
    let archived = root.path().join("archived_sessions");
    fs::create_dir_all(&live)?;
    fs::create_dir_all(&archived)?;
    fs::write(live.join("live.jsonl"), "{}\n")?;
    fs::write(archived.join("old.jsonl"), "{}\n")?;
    fs::write(archived.join("ignored.txt"), "{}\n")?;

    let result = discover(Some(root.path()));

    assert_eq!(result.files.len(), 2);
    assert_eq!(result.missing_roots, 0);
    Ok(())
}

#[test]
fn missing_explicit_root_has_no_fallback_files() -> Result<()> {
    let root = tempfile::tempdir()?;
    let result = discover(Some(&root.path().join("missing")));

    assert!(result.files.is_empty());
    assert_eq!(result.missing_roots, 1);
    Ok(())
}

#[test]
fn a_symlink_back_to_an_ancestor_is_not_walked() -> Result<()> {
    let root = tempfile::tempdir()?;
    let day = root.path().join("sessions/2026/10/14");
    fs::create_dir_all(&day)?;
    fs::write(day.join("rollout.jsonl"), "{}\n")?;
    std::os::unix::fs::symlink(root.path().join("sessions"), day.join("loop"))?;
    std::os::unix::fs::symlink(
        root.path().join("sessions/2026"),
        root.path().join("sessions/up"),
    )?;

    let result = discover(Some(root.path()));

    assert_eq!(result.files, vec![day.join("rollout.jsonl")]);
    assert_eq!(result.unreadable_directories, 0);
    Ok(())
}

#[test]
fn a_symlink_out_of_the_tree_is_not_entered_but_a_file_link_is_kept() -> Result<()> {
    let root = tempfile::tempdir()?;
    let outside = tempfile::tempdir()?;
    fs::write(outside.path().join("elsewhere.jsonl"), "{}\n")?;
    let sessions = root.path().join("sessions");
    fs::create_dir_all(&sessions)?;
    std::os::unix::fs::symlink(outside.path(), sessions.join("dir-link"))?;
    std::os::unix::fs::symlink(
        outside.path().join("elsewhere.jsonl"),
        sessions.join("file-link.jsonl"),
    )?;

    let result = discover(Some(root.path()));

    assert_eq!(result.files, vec![sessions.join("file-link.jsonl")]);
    Ok(())
}

#[test]
fn the_walk_stops_below_the_depth_bound() -> Result<()> {
    let root = tempfile::tempdir()?;
    let within = root.path().join("a/b/c/d");
    let beyond = within.join("e");
    fs::create_dir_all(&beyond)?;
    fs::write(within.join("kept.jsonl"), "{}\n")?;
    fs::write(beyond.join("dropped.jsonl"), "{}\n")?;

    let mut result = CodexDiscovery::default();
    collect_jsonl(root.path(), &mut result);

    assert_eq!(result.files, vec![within.join("kept.jsonl")]);
    assert_eq!(result.unreadable_directories, 0);
    Ok(())
}
