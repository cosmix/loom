//! `loom plan verify` as a function: a [`VerifyReport`] for a plan's text, without printing or
//! exiting. The CLI serialises the report for `--json`; the planning server validates proposed
//! text with the same checks.

use serde::Serialize;
use std::path::Path;

use super::{checks, find_repo_root, inputs};
use crate::plan::parser::{
    extract_plan_name, extract_yaml_metadata_with_ranges, ExtractedMetadata,
};
use crate::plan::schema::{base_tree, LoomMetadata, ValidationError};

const MAX_FILE_BYTES: u64 = 1_048_576; // 1 MiB

#[derive(Debug, Clone, Serialize)]
pub struct VerifyPlan {
    pub id: Option<String>,
    pub name: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyError {
    pub stage_id: Option<String>,
    pub message: String,
}

impl From<ValidationError> for VerifyError {
    fn from(error: ValidationError) -> Self {
        Self {
            stage_id: error.stage_id,
            message: error.message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct VerifyWarnings {
    pub structural: Vec<String>,
    pub knowledge: Vec<String>,
    pub sandbox: Vec<String>,
    /// Criteria that already pass on the untouched tree (see `base_tree`).
    pub baseline: Vec<String>,
}

impl VerifyWarnings {
    /// Every bucket with its human-output heading, in display order.
    pub fn sections(&self) -> [(&'static str, &[String]); 4] {
        [
            ("Structural", self.structural.as_slice()),
            ("Knowledge", self.knowledge.as_slice()),
            ("Sandbox", self.sandbox.as_slice()),
            ("Baseline", self.baseline.as_slice()),
        ]
    }

    /// Warnings across every bucket; `--strict` fails on any of them.
    pub fn total(&self) -> usize {
        self.sections().iter().map(|(_, items)| items.len()).sum()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VerifyStageLevel {
    pub id: String,
    pub name: String,
    pub stage_type: String,
    pub dependencies: Vec<String>,
}

/// Where a YAML parse error sits, 1-based, in the whole plan file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct YamlLocation {
    pub line: u64,
    pub column: u64,
}

/// Whether the text has a loom METADATA block with a YAML fence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataPresence {
    Present,
    Missing,
}

/// The `loom plan verify --json` document; the skipped fields are for in-process callers.
#[derive(Debug, Clone, Serialize)]
pub struct VerifyReport {
    pub plan: VerifyPlan,
    pub valid: bool,
    pub errors: Vec<VerifyError>,
    pub warnings: VerifyWarnings,
    /// Checks that were skipped, and why; never counted as warnings.
    pub notes: Vec<String>,
    pub levels: Vec<Vec<VerifyStageLevel>>,
    #[serde(skip)]
    pub yaml_location: Option<YamlLocation>,
    #[serde(skip)]
    pub metadata: MetadataPresence,
    /// Verification stopped before validation ran (unreadable file, no metadata block, YAML
    /// that does not parse); `errors` then holds that one reason.
    #[serde(skip)]
    pub stopped: bool,
}

impl VerifyReport {
    /// A report for verification that stopped before validation ran: one error, nothing else.
    fn stopped(source: String, message: String) -> Self {
        Self {
            plan: VerifyPlan {
                id: None,
                name: None,
                source,
            },
            valid: false,
            errors: vec![VerifyError {
                stage_id: None,
                message,
            }],
            warnings: VerifyWarnings::default(),
            notes: Vec::new(),
            levels: Vec::new(),
            yaml_location: None,
            metadata: MetadataPresence::Present,
            stopped: true,
        }
    }
}

/// A plan file `verify_path` refused before reading its text: absent, too large, or not UTF-8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EarlyFailure {
    pub source: String,
    pub message: String,
}

impl From<EarlyFailure> for VerifyReport {
    fn from(failure: EarlyFailure) -> Self {
        Self::stopped(failure.source, failure.message)
    }
}

/// Verify `text` as the content of `plan_path`. Input and base-tree checks run against
/// `repo_root` when it is given; the plan's own references come from `text`, never the disk.
pub fn verify_text(repo_root: Option<&Path>, plan_path: &Path, text: &str) -> VerifyReport {
    let source = plan_path.to_string_lossy().to_string();
    let extracted = match extract_yaml_metadata_with_ranges(text) {
        Ok(extracted) => extracted,
        Err(e) => {
            return VerifyReport {
                metadata: MetadataPresence::Missing,
                ..VerifyReport::stopped(source, e.to_string())
            }
        }
    };
    let metadata: LoomMetadata = match crate::yaml::from_str(&extracted.yaml) {
        Ok(metadata) => metadata,
        Err(e) => {
            return VerifyReport {
                yaml_location: e
                    .location()
                    .map(|at| file_location(text, &extracted, at.line(), at.column())),
                ..VerifyReport::stopped(source, format!("YAML parse error: {e}"))
            }
        }
    };
    let plan = VerifyPlan {
        id: plan_path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from),
        name: extract_plan_name(text).ok(),
        source,
    };
    validate(repo_root, plan_path, text, &metadata, plan)
}

/// The `loom:` metadata of a plan's text; `None` when the block is absent or does not parse.
pub(super) fn parse_metadata(text: &str) -> Option<LoomMetadata> {
    let extracted = extract_yaml_metadata_with_ranges(text).ok()?;
    crate::yaml::from_str(&extracted.yaml).ok()
}

/// Verify the plan file at `path`, with the repository root found by walking up from it.
pub fn verify_path(path: &Path) -> Result<VerifyReport, EarlyFailure> {
    let text = read_plan(path)?;
    let repo_root = plan_repo_root(path)?;
    Ok(verify_text(repo_root.as_deref(), path, &text))
}

/// The plan's text after the existence, size and UTF-8 checks.
fn read_plan(path: &Path) -> Result<String, EarlyFailure> {
    let fail = |message: String| EarlyFailure {
        source: path.to_string_lossy().to_string(),
        message,
    };
    if !path.is_file() {
        return Err(fail(format!("Plan file not found: {}", path.display())));
    }
    let file_len = std::fs::metadata(path)
        .map_err(|e| fail(format!("Failed to stat {}: {e}", path.display())))?
        .len();
    if file_len > MAX_FILE_BYTES {
        return Err(fail(format!(
            "Plan file too large: {file_len} bytes (limit: {MAX_FILE_BYTES} bytes)"
        )));
    }
    std::fs::read_to_string(path)
        .map_err(|e| fail(format!("Failed to read {}: {e}", path.display())))
}

/// The repository holding `path`, if any.
pub(super) fn plan_repo_root(path: &Path) -> Result<Option<std::path::PathBuf>, EarlyFailure> {
    let absolute = std::path::absolute(path).map_err(|e| EarlyFailure {
        source: path.to_string_lossy().to_string(),
        message: format!("Failed to resolve {}: {e}", path.display()),
    })?;
    Ok(find_repo_root(&absolute))
}

/// Every check over parsed metadata.
fn validate(
    repo_root: Option<&Path>,
    plan_path: &Path,
    text: &str,
    metadata: &LoomMetadata,
    plan: VerifyPlan,
) -> VerifyReport {
    let mut errors: Vec<VerifyError> = Vec::new();
    let mut warnings = VerifyWarnings::default();
    let mut levels = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    match crate::plan::schema::validate(metadata) {
        Err(errs) => errors.extend(errs.into_iter().map(VerifyError::from)),
        Ok(()) => {
            (warnings, levels) = checks::soft_checks(metadata, repo_root, &mut errors, &mut notes);
        }
    }
    if let Some(root) = repo_root {
        let (input_errors, input_notes) = match std::path::absolute(plan_path) {
            Ok(plan) => inputs::input_findings(root, &plan, Some(text)),
            Err(e) => {
                let message = format!("Cannot check plan inputs: {e}");
                (vec![inputs::error(message)], Vec::new())
            }
        };
        errors.extend(input_errors);
        notes.extend(input_notes);
    }
    // Base-tree evaluation runs whether or not the schema validates, so one
    // pass reports a plan's hazard errors and its already-green criteria.
    let baseline = base_tree::check_base_tree(&metadata.loom.stages, repo_root);
    warnings.baseline = baseline.warnings;
    notes.extend(baseline.note);
    VerifyReport {
        plan,
        valid: errors.is_empty(),
        errors,
        warnings,
        notes,
        levels,
        yaml_location: None,
        metadata: MetadataPresence::Present,
        stopped: false,
    }
}

/// Map a 1-based position in the trimmed YAML body to the same position in the whole file.
fn file_location(
    text: &str,
    extracted: &ExtractedMetadata,
    line: u64,
    column: u64,
) -> YamlLocation {
    // The body follows the backticks and the `yaml` hint; `yaml` is that body, trimmed.
    let fence = &text[extracted.yaml_fence_range.clone()];
    let body = &fence[fence.bytes().take_while(|&b| b == b'`').count() + "yaml".len()..];
    let start = extracted.yaml_fence_range.start + fence.len() - body.trim_start().len();
    let before = &text[..start];
    let lines_before = before.bytes().filter(|&b| b == b'\n').count() as u64;
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let indent = before[line_start..].chars().count() as u64;
    YamlLocation {
        line: lines_before + line,
        column: if line == 1 { column + indent } else { column },
    }
}
