//! `loom knowledge check` — report knowledge-base diagnostics WITHOUT ever
//! opening the context store, which is what makes it safe to run as a loom
//! stage's acceptance criterion.
//!
//! `loom knowledge sync` resolves through `context::retrieve::resolve_roots`
//! (`commands/knowledge/context.rs:29`), which calls `ContextStore::open`
//! (`context/store.rs:49`). `open` itself only COMPUTES the cache root: it
//! follows `WorkDir::main_project_root` OUT of a worktree, through the
//! state directory symlink, to the MAIN repository, and joins `.loom/cache`. The
//! write happens later, when `sync`'s `refresh` (`context/refresh.rs:218`)
//! calls `ContextStore::save_catalog` (`context/store.rs:108`) against that
//! root — a write that escapes worktree isolation, and both settings
//! emitters strip `.loom` from `allow_write`, so `sync` can never sit in a
//! stage's acceptance list without tripping the sandbox.
//! `catalog::build(root: &Path)` (`fs/knowledge/catalog.rs`) is pure: it reads
//! the tree and returns diagnostics, writing nothing. `check` resolves ONLY
//! the knowledge root — never the context store — and calls `catalog::build`
//! directly. `skills/loom-plan-writer/SKILL.md` already names `loom knowledge
//! check` as knowledge-distill's acceptance criterion, so this command fills a
//! contract that already ships; do not "simplify" it back into
//! `context::resolve()`.

use super::check_lines::{decorated_issue_line, issue_line};
use crate::fs::knowledge::catalog::{
    self, BaselineComparison, Catalog, CatalogIssue, CheckBaseline,
};
use crate::fs::knowledge::KnowledgeDir;
use crate::fs::work_dir::WorkDir;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};

/// Flags of `loom knowledge check`.
#[derive(Debug, Default)]
pub struct CheckOptions {
    pub strict: bool,
    pub strict_evidence: bool,
    pub json: bool,
    /// Structural issues recorded in this file do not fail `--strict`.
    pub baseline: Option<PathBuf>,
    /// Write the current structural issue set to this file and exit 0.
    pub write_baseline: Option<PathBuf>,
}

/// Report the knowledge base's diagnostics. Resolves the knowledge root
/// read-only (see the module doc) and never initializes or mutates it.
///
/// `--strict` rejects structural diagnostics, or with `--baseline` only those
/// the baseline does not record; `--strict-evidence` additionally rejects
/// changed or unavailable declared source evidence.
pub fn check(options: CheckOptions) -> Result<()> {
    let root = knowledge_root()?;
    if let Some(path) = &options.write_baseline {
        reject_write_baseline_conflicts(&options)?;
        return write_baseline(&root, path);
    }
    if !root.exists() {
        report_missing_root(&root, options.json)?;
        if options.strict_evidence {
            strict_failure(1, &root);
        }
        return Ok(());
    }

    let baseline = match &options.baseline {
        Some(path) => Some((path.as_path(), CheckBaseline::read(path)?)),
        None => None,
    };
    let catalog = catalog::build(&root)?;
    let comparison = baseline
        .as_ref()
        .map(|(path, baseline)| (*path, baseline.compare(&catalog.issues)));
    if options.json {
        print_json(&root, &catalog, comparison.as_ref())?;
    } else {
        print_human(&root, &catalog);
        if let Some((path, comparison)) = &comparison {
            print_baseline_report(path, comparison);
        }
    }

    let structural = comparison.as_ref().map_or_else(
        || strict_issue_count(&catalog.issues),
        |(_, comparison)| comparison.new.len(),
    );
    let failure_count = strict_failure_count(
        options.strict,
        options.strict_evidence,
        structural,
        &catalog.issues,
    );
    if failure_count > 0 {
        strict_failure(failure_count, &root);
    }
    Ok(())
}

/// `--write-baseline` writes and exits before any other flag is honoured, so
/// combining it with a flag that changes what gets reported or how the run
/// fails must error instead of silently ignoring that flag.
fn reject_write_baseline_conflicts(options: &CheckOptions) -> Result<()> {
    if options.strict || options.strict_evidence || options.json || options.baseline.is_some() {
        bail!(
            "--write-baseline cannot be combined with --strict, --strict-evidence, --json, or --baseline"
        );
    }
    Ok(())
}

/// Record every structural issue of the current tree in `path`. A missing
/// knowledge root records nothing.
fn write_baseline(root: &Path, path: &Path) -> Result<()> {
    let issues = if root.exists() {
        catalog::build(root)?.issues
    } else {
        Vec::new()
    };
    let text = catalog::render_baseline(&issues);
    std::fs::write(path, &text)
        .with_context(|| format!("Failed to write baseline {}", path.display()))?;
    let entries = text.lines().filter(|line| !line.starts_with('#')).count();
    println!(
        "{} Wrote {entries} structural issue(s) to {}",
        "✓".green().bold(),
        path.display()
    );
    Ok(())
}

/// One line per structural issue the baseline does not record, and one
/// "baseline can be tightened" line when recorded issues are gone.
fn baseline_report(path: &Path, comparison: &BaselineComparison<'_>) -> Vec<String> {
    let mut lines: Vec<String> = comparison
        .new
        .iter()
        .map(|issue| format!("new since baseline: {}", issue_line(issue)))
        .collect();
    if !comparison.tightenable.is_empty() {
        lines.push(format!(
            "baseline can be tightened: {} recorded issue(s) in {} no longer occur - regenerate with `loom knowledge check --write-baseline {}`",
            comparison.tightenable.len(),
            path.display(),
            path.display()
        ));
    }
    lines
}

fn print_baseline_report(path: &Path, comparison: &BaselineComparison<'_>) {
    for line in baseline_report(path, comparison) {
        println!("{line}");
    }
}

fn strict_failure(count: usize, root: &Path) -> ! {
    eprintln!(
        "loom knowledge check: FAIL - {count} issue(s) found under {}",
        root.display()
    );
    std::process::exit(1)
}

/// `doc/loom/knowledge` under the current project root, resolved WITHOUT
/// `KnowledgeDir::initialize()` and WITHOUT touching the context store.
///
/// Deliberately not `super::open_knowledge_dir()`: that helper initializes a
/// missing directory, which is exactly the mutation a read-only diagnostic
/// command must not perform.
fn knowledge_root() -> Result<std::path::PathBuf> {
    let work_dir = WorkDir::new(".")?;
    let project_root = work_dir
        .project_root()
        .context("Could not determine project root")?;
    Ok(KnowledgeDir::new(project_root).root().to_path_buf())
}

fn report_missing_root(root: &Path, json: bool) -> Result<()> {
    if json {
        // Reuses `json_payload` against an empty catalog rather than
        // hand-writing a second `{root, issues, count}` literal, so the two
        // JSON shapes cannot drift apart.
        let empty = Catalog {
            revision: String::new(),
            chunks: Vec::new(),
            issues: Vec::new(),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&json_payload(root, &empty))?
        );
    } else {
        println!(
            "{} No knowledge directory at {}",
            "─".dimmed(),
            root.display()
        );
    }
    Ok(())
}

/// The JSON payload's shape. Issues are structured data a machine parses, so
/// unlike [`issue_line`] this must NOT route any field through
/// `inline_safe` — flattening is for the human-readable stdout line only.
fn json_payload(root: &Path, catalog: &Catalog) -> serde_json::Value {
    let issues: Vec<_> = catalog
        .issues
        .iter()
        .filter(|issue| !issue.is_review_only())
        .collect();
    let review: Vec<_> = catalog
        .issues
        .iter()
        .filter(|issue| issue.is_review_only())
        .collect();
    serde_json::json!({
        "root": root,
        "issues": issues,
        "review": review,
        "count": strict_issue_count(&catalog.issues),
        "evidence": catalog::evidence_summary(root, &catalog.issues),
    })
}

fn strict_issue_count(issues: &[CatalogIssue]) -> usize {
    issues
        .iter()
        .filter(|issue| !issue.is_review_only())
        .count()
}

/// Issues that fail the run: `structural` is every structural issue, or
/// with a baseline only the unrecorded ones.
fn strict_failure_count(
    strict: bool,
    strict_evidence: bool,
    structural: usize,
    issues: &[CatalogIssue],
) -> usize {
    if !(strict || strict_evidence) {
        return 0;
    }
    if strict_evidence {
        structural + evidence_issue_count(issues)
    } else {
        structural
    }
}

fn evidence_issue_count(issues: &[CatalogIssue]) -> usize {
    issues
        .iter()
        .filter(|issue| {
            matches!(
                issue,
                CatalogIssue::EvidenceChanged { .. } | CatalogIssue::EvidenceUnavailable { .. }
            )
        })
        .count()
}

fn print_json(
    root: &Path,
    catalog: &Catalog,
    comparison: Option<&(&Path, BaselineComparison<'_>)>,
) -> Result<()> {
    let mut payload = json_payload(root, catalog);
    if let Some((path, comparison)) = comparison {
        payload["baseline"] = serde_json::json!({
            "path": path,
            "new": comparison.new,
            "tightenable": comparison.tightenable,
        });
    }
    println!("{}", serde_json::to_string_pretty(&payload)?);
    Ok(())
}

fn print_human(root: &Path, catalog: &Catalog) {
    println!("{}", human_report(root, catalog));
}

/// Build the human-readable report as a single string, mirroring
/// [`json_payload`]'s split from [`print_json`] — `print_human` stays a thin
/// `println!` wrapper, and this is what tests assert against so a deleted
/// or garbled report body fails a test instead of going unnoticed.
fn human_report(root: &Path, catalog: &Catalog) -> String {
    let evidence = catalog::evidence_summary(root, &catalog.issues);
    if catalog.issues.is_empty() {
        return format!(
            "{} Knowledge base at {} is clean ({}/{} evidence files current; {} unassessed)",
            "✓".green().bold(),
            root.display(),
            evidence.current,
            evidence.declared,
            evidence.unassessed
        );
    }
    let mut lines: Vec<String> = catalog.issues.iter().map(decorated_issue_line).collect();
    let strict_count = strict_issue_count(&catalog.issues);
    let review_count = catalog.issues.len() - strict_count;
    lines.push(format!(
        "{} {strict_count} issue(s), {review_count} review note(s) found under {}",
        "!".yellow().bold(),
        root.display()
    ));
    lines.push(format!(
        "evidence: {} ({}/{} current, {} changed, {} unavailable, {} unassessed)",
        evidence.status,
        evidence.current,
        evidence.declared,
        evidence.changed,
        evidence.unavailable,
        evidence.unassessed
    ));
    lines.join("\n")
}

#[cfg(test)]
#[path = "tests_check.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_check_evidence.rs"]
mod tests_evidence;

#[cfg(test)]
#[path = "tests_check_baseline.rs"]
mod tests_baseline;
