use super::*;

/// Finish prepared transactions, remove orphan snapshots, and repair the latest
/// committed field if either live record has drifted. All writes hold the lock.
pub fn verify_plan_versions_consistency(plan_path: &Path, work_dir: &Path) -> Result<usize> {
    let dir = plan_versions_dir(work_dir);
    if !dir.exists() {
        return Ok(0);
    }
    let _lock = PlanVersionsLock::acquire(work_dir)?;
    let rows = read_audit_rows(work_dir)?;
    let (snapshots, mut actions) = remove_orphans(&dir, &rows)?;
    let plan_path = if plan_path.exists() {
        plan_path.to_path_buf()
    } else {
        resolve_plan_path(work_dir).unwrap_or_else(|_| plan_path.to_path_buf())
    };
    actions += super::transaction::finish_pending(&plan_path, work_dir)?;
    let latest = rows
        .iter()
        .filter(|row| snapshots.contains(&row.version))
        .max_by_key(|row| row.version);
    if let Some(row) = latest {
        actions += repair_latest(&plan_path, work_dir, row)?;
    }
    Ok(actions)
}

fn remove_orphans(dir: &Path, rows: &[AuditRow]) -> Result<(Vec<u64>, usize)> {
    let mut snapshots = Vec::new();
    let mut actions = 0;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name
            .strip_suffix(".md.tmp")
            .is_some_and(|stem| stem.parse::<u64>().is_ok())
        {
            fs::remove_file(&path)?;
            actions += 1;
        } else if let Some(version) = name
            .strip_suffix(".md")
            .and_then(|stem| stem.parse::<u64>().ok())
        {
            if rows.iter().any(|row| row.version == version) {
                snapshots.push(version);
            } else {
                fs::remove_file(&path)?;
                actions += 1;
            }
        }
    }
    Ok((snapshots, actions))
}

fn repair_latest(plan_path: &Path, work_dir: &Path, row: &AuditRow) -> Result<usize> {
    let snapshot =
        fs::read_to_string(plan_versions_dir(work_dir).join(snapshot_filename(row.version)))?;
    let project_root = resolve_project_root(work_dir, plan_path);
    let changed = match amendment_catch_up::reconcile_amended_field(
        plan_path,
        &project_root,
        &snapshot,
        &row.stage_id,
        row.field,
    ) {
        Ok(changed) => changed,
        Err(reason) => {
            amendment_catch_up::warn_skip(plan_path, &row.stage_id, reason);
            return Ok(0);
        }
    };
    Ok(usize::from(changed) + repair_stage(work_dir, row, &snapshot)?)
}

fn repair_stage(work_dir: &Path, row: &AuditRow, snapshot: &str) -> Result<usize> {
    let extracted = extract_yaml_metadata_with_ranges(snapshot)?;
    let metadata = parse_and_validate(&extracted.yaml)?;
    let definition = metadata
        .loom
        .stages
        .iter()
        .find(|s| s.id == row.stage_id)
        .context("Stage missing from amendment snapshot")?;
    let current = load_stage(&row.stage_id, work_dir)?;
    if stage_field_matches(&current, definition, row.field) {
        return Ok(0);
    }
    update_stage(&row.stage_id, work_dir, |stage| {
        sync_stage_from_definition(stage, definition, row.field);
        Ok(())
    })?;
    Ok(1)
}
