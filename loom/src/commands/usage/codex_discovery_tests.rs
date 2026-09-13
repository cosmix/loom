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
