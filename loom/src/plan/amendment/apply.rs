use super::transaction::{finish_pending, finish_recorded, inject_failure};
use super::*;

/// Prepare an audited snapshot, then finish both live records under the version lock.
pub fn apply_amendment(
    plan_path: &Path,
    work_dir: &Path,
    request: AmendmentRequest,
) -> Result<AmendmentResult> {
    let _lock = PlanVersionsLock::acquire(work_dir)?;
    let plan_path = checked_plan_path(plan_path, work_dir)?;
    finish_pending(&plan_path, work_dir)?;
    let rows = read_audit_rows(work_dir)?;
    if let Some(row) = rows.iter().find(|row| {
        request.dispute_id.is_some()
            && row.stage_id == request.stage_id
            && row.dispute_id == request.dispute_id
    }) {
        return amendment_result(work_dir, row);
    }
    let content = prepare_content(&plan_path, work_dir, &rows, &request)?;
    let row = prepare_record(work_dir, &request, &content)?;
    inject_failure(work_dir, "after-audit")?;
    finish_recorded(&plan_path, work_dir, &row)?;
    amendment_result(work_dir, &row)
}

fn checked_plan_path(plan_path: &Path, work_dir: &Path) -> Result<PathBuf> {
    let plan_path = if plan_path.exists() {
        plan_path.to_path_buf()
    } else {
        resolve_plan_path(work_dir)?
    };
    let root = resolve_project_root(work_dir, &plan_path).canonicalize()?;
    let parent = plan_path
        .parent()
        .context("plan_path has no parent")?
        .canonicalize()?;
    if !parent.starts_with(&root) {
        bail!(
            "refusing replace — plan {} not under project_root {}",
            plan_path.display(),
            root.display()
        );
    }
    Ok(plan_path)
}

fn prepare_content(
    plan_path: &Path,
    work_dir: &Path,
    rows: &[AuditRow],
    request: &AmendmentRequest,
) -> Result<String> {
    let content = fs::read_to_string(plan_path)?;
    let extracted = extract_yaml_metadata_with_ranges(&content)?;
    let mut metadata = parse_and_validate(&extracted.yaml)?;
    let cap = metadata
        .loom
        .adjudication
        .clone()
        .unwrap_or_default()
        .max_amendments_per_stage;
    let count = count_verdicts(rows, request);
    if count >= cap && !continues_counted_verdict(rows, request) {
        bail!(
            "{}: stage '{}' has reached the amendment cap ({} of {})",
            AMENDMENT_CAP_ERROR_PREFIX,
            request.stage_id,
            count,
            cap
        );
    }
    load_stage(&request.stage_id, work_dir)?;
    let definition = metadata
        .loom
        .stages
        .iter_mut()
        .find(|s| s.id == request.stage_id)
        .with_context(|| format!("Stage '{}' not found in plan", request.stage_id))?;
    let value = parse_amendment_value(request)?;
    validate_index(definition, request)?;
    apply_patch_to_stage_def(definition, request.field, &request.patch, &value)?;
    if let Err(errors) = crate::plan::schema::validate(&metadata) {
        bail!(
            "Amendment would produce an invalid plan: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        );
    }
    let yaml = serialize_loom_metadata(&metadata)?;
    Ok(splice_metadata_yaml(&content, &extracted, &yaml))
}

/// The part of a dispute id before the patch suffix: `"7.2"` belongs to verdict `"7"`.
fn base_dispute_id(id: &str) -> &str {
    id.split('.').next().unwrap_or(id)
}

fn stage_rows<'a>(
    rows: &'a [AuditRow],
    stage_id: &'a str,
) -> impl Iterator<Item = &'a AuditRow> + 'a {
    rows.iter().filter(move |row| row.stage_id == stage_id)
}

/// Amendments spent against the cap: one per verdict, so a multi-patch verdict
/// (`<id>.<k>`) counts once. A row without a dispute id counts on its own.
fn count_verdicts(rows: &[AuditRow], request: &AmendmentRequest) -> u32 {
    let mut seen = std::collections::HashSet::new();
    let mut count = 0u32;
    for row in stage_rows(rows, &request.stage_id) {
        match row.dispute_id.as_deref() {
            Some(id) => count += u32::from(seen.insert(base_dispute_id(id))),
            None => count += 1,
        }
    }
    count
}

/// True when an earlier patch of the request's own verdict is already recorded,
/// so the cap was checked before that verdict's first patch and the remaining
/// patches must finish it.
fn continues_counted_verdict(rows: &[AuditRow], request: &AmendmentRequest) -> bool {
    let Some(base) = request.dispute_id.as_deref().map(base_dispute_id) else {
        return false;
    };
    stage_rows(rows, &request.stage_id)
        .any(|row| row.dispute_id.as_deref().map(base_dispute_id) == Some(base))
}

fn validate_index(
    definition: &crate::plan::schema::StageDefinition,
    request: &AmendmentRequest,
) -> Result<()> {
    let len = current_field_len(definition, request.field);
    let index = request.patch.index();
    let valid = match request.patch {
        AmendmentPatch::Insert { .. } => index <= len,
        _ => index < len,
    };
    if !valid {
        bail!("Amendment index out of bounds: stage '{}' field {:?} has {} entries; index {} not permitted",
            request.stage_id, request.field, len, index);
    }
    Ok(())
}

fn prepare_record(work_dir: &Path, request: &AmendmentRequest, content: &str) -> Result<AuditRow> {
    let version = compute_next_version(work_dir)?;
    let dir = ensure_plan_versions_dir(work_dir)?;
    let dirfd = safe_fs::safe_open_dirfd(&dir)?;
    let tmp = PathBuf::from(snapshot_tmp_filename(version));
    let snapshot = PathBuf::from(snapshot_filename(version));
    safe_fs::safe_create_new_in_workdir(dirfd.as_raw_fd(), &tmp, content.as_bytes())?;
    safe_fs::safe_rename_in_workdir(dirfd.as_raw_fd(), &tmp, &snapshot)?;
    let row = AuditRow {
        version,
        stage_id: request.stage_id.clone(),
        field: request.field,
        patch_op: match request.patch {
            AmendmentPatch::Replace { .. } => "replace",
            AmendmentPatch::Insert { .. } => "insert",
            AmendmentPatch::Delete { .. } => "delete",
        },
        index: request.patch.index(),
        applied_at: Utc::now(),
        dispute_id: request.dispute_id.clone(),
        reason: request.reason.clone(),
    };
    append_audit_row(work_dir, &row)?;
    Ok(row)
}

fn amendment_result(work_dir: &Path, row: &AuditRow) -> Result<AmendmentResult> {
    Ok(AmendmentResult {
        version: row.version,
        stage_id: row.stage_id.clone(),
        field: row.field,
        snapshot_path: plan_versions_dir(work_dir).join(snapshot_filename(row.version)),
        amendments_applied: count_amendments_for_stage(work_dir, &row.stage_id)?,
        applied_at: row.applied_at,
    })
}
