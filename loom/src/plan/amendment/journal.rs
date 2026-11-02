use super::*;

/// Path to `.loom/work/plan_versions/`.
///
/// Returned as an absolute path when `work_dir` is absolute.
pub fn plan_versions_dir(work_dir: &Path) -> PathBuf {
    work_dir.join(PLAN_VERSIONS_DIR_NAME)
}

/// Compute the snapshot filename for a given version.
pub(super) fn snapshot_filename(version: u64) -> String {
    format!("{version}.md")
}

/// Compute the snapshot tmp filename for a given version.
pub(super) fn snapshot_tmp_filename(version: u64) -> String {
    format!("{version}.md.tmp")
}

/// Ensure `.loom/work/plan_versions/` exists.
///
/// Idempotent.
pub(super) fn ensure_plan_versions_dir(work_dir: &Path) -> Result<PathBuf> {
    let dir = plan_versions_dir(work_dir);
    if !dir.exists() {
        fs::create_dir_all(&dir).with_context(|| {
            format!(
                "Failed to create plan_versions directory at {}",
                dir.display()
            )
        })?;
    }
    Ok(dir)
}

/// Open (or create) the plan-versions lock file and hold an exclusive flock
/// for the lifetime of the returned guard.
pub(super) struct PlanVersionsLock {
    _file: fs::File,
}

impl PlanVersionsLock {
    pub(super) fn acquire(work_dir: &Path) -> Result<Self> {
        let dir = ensure_plan_versions_dir(work_dir)?;
        let lock_path = dir.join(PLAN_VERSIONS_LOCK_NAME);
        // Use OpenOptions directly (not safe_fs) — the lock file is created
        // with O_CREAT and we want a long-lived handle for flock; safe_fs's
        // helpers all close-on-return.
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| {
                format!("Failed to open plan-versions lock {}", lock_path.display())
            })?;
        file.lock_exclusive().with_context(|| {
            format!(
                "Failed to acquire plan-versions lock {}",
                lock_path.display()
            )
        })?;
        Ok(Self { _file: file })
    }
}

/// One row in `.loom/work/plan_versions/audit.md`.
///
/// The audit log is a markdown table whose rows are append-only. Writers
/// atomically publish the full table so lock-free policy readers see whole rows.
/// We keep the schema deliberately simple so a human can read the file
/// without tooling: `version | stage_id | field | patch_op | index |
/// applied_at | dispute_id | reason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AuditRow {
    pub(super) version: u64,
    pub(super) stage_id: String,
    pub(super) field: AmendmentField,
    pub(super) patch_op: &'static str,
    pub(super) index: usize,
    pub(super) applied_at: DateTime<Utc>,
    pub(super) dispute_id: Option<String>,
    pub(super) reason: Option<String>,
}

impl AuditRow {
    pub(super) fn to_markdown_row(&self) -> String {
        // The audit log uses a markdown table parsed by splitting on '|'.
        // We aggressively sanitise the reason cell so a stray pipe or
        // newline can never confuse the parser: pipes become '/' and
        // newlines become spaces. Round-trip fidelity of the reason text
        // is intentionally sacrificed for parser simplicity.
        let reason = self
            .reason
            .as_deref()
            .unwrap_or("")
            .replace('|', "/")
            .replace('\n', " ");
        let dispute = self.dispute_id.as_deref().unwrap_or("");
        let field = match self.field {
            AmendmentField::Acceptance => "acceptance",
            AmendmentField::Wiring => "wiring",
            AmendmentField::WiringTests => "wiring_tests",
            AmendmentField::Contracts => "contracts",
            AmendmentField::Harness => "harness",
            AmendmentField::Reachable => "reachable",
        };
        format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} |\n",
            self.version,
            self.stage_id,
            field,
            self.patch_op,
            self.index,
            self.applied_at.to_rfc3339(),
            dispute,
            reason,
        )
    }
}

pub(super) const AUDIT_HEADER: &str = "# Plan Amendment Audit\n\n\
| version | stage_id | field | op | index | applied_at | dispute_id | reason |\n\
|---------|----------|-------|----|-------|------------|------------|--------|\n";

/// Read every audit row from `.loom/work/plan_versions/audit.md` (empty if missing).
/// A present journal must have the published table framing. Its rows are
/// authoritative evidence for recovery, so damaged rows cannot be skipped.
pub(super) fn read_audit_rows(work_dir: &Path) -> Result<Vec<AuditRow>> {
    let dir = plan_versions_dir(work_dir);
    let audit_path = dir.join(AUDIT_FILE_NAME);
    if !audit_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(&audit_path)
        .with_context(|| format!("Failed to read audit log {}", audit_path.display()))?;
    let lines = audit_data_lines(&content, &audit_path)?;
    let mut rows = Vec::new();
    for (number, line) in lines {
        let row = parse_audit_row(line).with_context(|| {
            format!(
                "Malformed audit log {} at line {}",
                audit_path.display(),
                number
            )
        })?;
        rows.push(row);
    }
    if rows.is_empty() {
        bail!(
            "Malformed audit log {}: table has no amendment rows",
            audit_path.display()
        );
    }
    Ok(rows)
}

fn audit_data_lines<'a>(content: &'a str, audit_path: &Path) -> Result<Vec<(usize, &'a str)>> {
    // Accept CRLF and extra blank lines around the published table, but no
    // alternate table or prose after it: either could conceal a lost row.
    let lines: Vec<(usize, &str)> = content
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.is_empty())
        .collect();
    let header: Vec<&str> = AUDIT_HEADER
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    for (index, expected) in header.iter().enumerate() {
        let Some(&(number, actual)) = lines.get(index) else {
            bail!(
                "Malformed audit log {}: missing table header at line {}",
                audit_path.display(),
                content.lines().count() + 1
            );
        };
        if actual != *expected {
            bail!(
                "Malformed audit log {} at line {}: expected '{}'",
                audit_path.display(),
                number,
                expected
            );
        }
    }
    Ok(lines.into_iter().skip(header.len()).collect())
}

/// Append one row under the plan-version lock and atomically publish the table.
/// Completion reads this journal while holding the stage lock, so it cannot
/// acquire the plan-version lock without reversing the amendment lock order.
pub(super) fn append_audit_row(work_dir: &Path, row: &AuditRow) -> Result<()> {
    let dir = plan_versions_dir(work_dir);
    let audit_path = dir.join(AUDIT_FILE_NAME);
    let mut body = match fs::read_to_string(&audit_path) {
        Ok(body) => body,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AUDIT_HEADER.to_string(),
        Err(error) => return Err(error).context("Failed to read amendment audit log"),
    };
    body.push_str(&row.to_markdown_row());
    let dirfd = safe_fs::safe_open_dirfd(&dir)?;
    let temporary = Path::new("audit.md.tmp");
    safe_fs::safe_locked_write_in_workdir(dirfd.as_raw_fd(), temporary, body.as_bytes())?;
    super::transaction::inject_failure(work_dir, "before-audit-publish")?;
    safe_fs::safe_rename_in_workdir(dirfd.as_raw_fd(), temporary, Path::new(AUDIT_FILE_NAME))?;
    Ok(())
}

pub(super) fn compute_next_version(work_dir: &Path) -> Result<u64> {
    let rows = read_audit_rows(work_dir)?;
    let max = rows.iter().map(|r| r.version).max().unwrap_or(0);
    // Also account for any snapshot files (e.g. from a crash before audit
    // append) so we don't reuse a version.
    let dir = plan_versions_dir(work_dir);
    let mut max_file: u64 = 0;
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str() {
                if let Some(stem) = name.strip_suffix(".md") {
                    if let Ok(v) = stem.parse::<u64>() {
                        if v > max_file {
                            max_file = v;
                        }
                    }
                }
                if let Some(stem) = name.strip_suffix(".md.tmp") {
                    if let Ok(v) = stem.parse::<u64>() {
                        if v > max_file {
                            max_file = v;
                        }
                    }
                }
            }
        }
    }
    Ok(max.max(max_file) + 1)
}

fn parse_audit_row(line: &str) -> Result<AuditRow> {
    let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
    // Expected: [empty, version, stage_id, field, op, index, applied_at, dispute_id, reason, empty]
    if cells.len() != 10 || !cells[0].is_empty() || !cells[9].is_empty() {
        bail!("invalid table row structure");
    }
    let version: u64 = cells[1].parse().context("invalid version")?;
    if version == 0 {
        bail!("invalid version: zero");
    }
    if cells[2].is_empty() {
        bail!("empty stage ID");
    }
    let stage_id = cells[2].to_string();
    let field = field_from_name(cells[3]).context("invalid field")?;
    let patch_op = match cells[4] {
        "replace" => "replace",
        "insert" => "insert",
        "delete" => "delete",
        _ => bail!("invalid patch operation"),
    };
    let index: usize = cells[5].parse().context("invalid patch index")?;
    let applied_at = DateTime::parse_from_rfc3339(cells[6])
        .context("invalid applied_at timestamp")?
        .with_timezone(&Utc);
    let dispute_id = (!cells[7].is_empty()).then(|| cells[7].to_string());
    let reason = (!cells[8].is_empty()).then(|| cells[8].to_string());
    Ok(AuditRow {
        version,
        stage_id,
        field,
        patch_op,
        index,
        applied_at,
        dispute_id,
        reason,
    })
}
