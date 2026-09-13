use anyhow::Result;

use super::*;

#[test]
fn current_project_explicit_root_matches_work_dir() -> Result<()> {
    let root = tempfile::tempdir()?;
    let selected = forward_receipts_root(false, Some(root.path()), None, Some(root.path()))?;

    assert_eq!(selected, Some(root.path()));
    Ok(())
}

#[test]
fn current_project_rejects_foreign_explicit_root() -> Result<()> {
    let work_dir = tempfile::tempdir()?;
    let foreign_root = tempfile::tempdir()?;
    let selected = forward_receipts_root(
        false,
        Some(foreign_root.path()),
        None,
        Some(work_dir.path()),
    );

    assert!(selected.is_err());
    Ok(())
}

#[test]
fn current_project_rejects_explicit_root_without_work_dir() -> Result<()> {
    let explicit = tempfile::tempdir()?;
    let selected = forward_receipts_root(false, Some(explicit.path()), None, None);

    assert!(selected.is_err());
    Ok(())
}
