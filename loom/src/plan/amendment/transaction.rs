//! An audit row records prepared intent; the applied receipt follows both writes.
use super::*;

fn receipt_path(work_dir: &Path, version: u64) -> PathBuf {
    plan_versions_dir(work_dir).join(format!("{version}.applied"))
}

fn mark_applied(work_dir: &Path, row: &AuditRow) -> Result<()> {
    let dirfd = safe_fs::safe_open_dirfd(&plan_versions_dir(work_dir))?;
    let name = PathBuf::from(format!("{}.applied", row.version));
    safe_fs::safe_create_new_in_workdir(dirfd.as_raw_fd(), &name, b"applied\n")?;
    Ok(())
}

/// The caller holds the plan-version lock. A newer row for the same field
/// supersedes the older snapshot; an unrelated stage never supersedes it.
pub(super) fn finish_pending(plan_path: &Path, work_dir: &Path) -> Result<usize> {
    let mut rows = read_audit_rows(work_dir)?;
    rows.sort_by_key(|row| row.version);
    let mut actions = 0;
    for row in &rows {
        if receipt_path(work_dir, row.version).exists() {
            continue;
        }
        let latest = rows
            .iter()
            .rev()
            .find(|later| later.stage_id == row.stage_id && later.field == row.field)
            .expect("row itself matches");
        if !receipt_path(work_dir, latest.version).exists() {
            actions += finish_recorded(plan_path, work_dir, latest)?;
        }
        if row.version != latest.version {
            mark_applied(work_dir, row)?;
        }
    }
    Ok(actions)
}

/// Finish the recorded field value, never the original index-based patch.
pub(super) fn finish_recorded(plan_path: &Path, work_dir: &Path, row: &AuditRow) -> Result<usize> {
    let snapshot =
        fs::read_to_string(plan_versions_dir(work_dir).join(snapshot_filename(row.version)))?;
    let project_root = resolve_project_root(work_dir, plan_path);
    let changed = amendment_catch_up::reconcile_amended_field(
        plan_path,
        &project_root,
        &snapshot,
        &row.stage_id,
        row.field,
    )
    .map_err(|reason| anyhow::anyhow!("Cannot finish amendment {}: {reason}", row.version))?;
    inject_failure(work_dir, "after-plan")?;
    let extracted = extract_yaml_metadata_with_ranges(&snapshot)?;
    let metadata = parse_and_validate(&extracted.yaml)?;
    let definition = metadata
        .loom
        .stages
        .iter()
        .find(|s| s.id == row.stage_id)
        .context("Amended stage missing from recorded snapshot")?;
    let mut stage_changed = false;
    update_stage(&row.stage_id, work_dir, |stage| {
        stage_changed = !stage_field_matches(stage, definition, row.field);
        sync_stage_from_definition(stage, definition, row.field);
        Ok(())
    })?;
    mark_applied(work_dir, row)?;
    Ok(usize::from(changed) + usize::from(stage_changed))
}

#[cfg(test)]
thread_local! {
    static FAILURE: std::cell::RefCell<Option<(PathBuf, &'static str)>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(super) fn fail_once(work_dir: &Path, phase: &'static str) {
    FAILURE.with(|fault| *fault.borrow_mut() = Some((work_dir.to_path_buf(), phase)));
}

pub(super) fn inject_failure(_work_dir: &Path, _phase: &str) -> Result<()> {
    #[cfg(test)]
    FAILURE.with(|fault| {
        let mut fault = fault.borrow_mut();
        if fault
            .as_ref()
            .is_some_and(|(dir, phase)| dir == _work_dir && *phase == _phase)
        {
            fault.take();
            bail!("injected amendment failure at {_phase}");
        }
        Ok(())
    })?;
    Ok(())
}
