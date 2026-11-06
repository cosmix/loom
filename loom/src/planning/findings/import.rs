//! Importing a run review's open findings and questions into the ledger.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::Path;

use super::{entry_text, ledger_path, next_number, parse, title, FindingStatus};
use crate::context::source_graph::body_hash;
use crate::fs::locking::lock_plan;
use crate::fs::safe_read::read_to_string_bounded;
use crate::fs::safe_replace::{replace_no_follow, restore_no_follow};
use crate::planning::discover::MAX_DOC_BYTES;
use crate::planning::implements;
use crate::planning::kinds::{self, DocKind};
use crate::planning::paths::symlink_component;
use crate::planning::run_review::{review_items, ReviewSection};
use crate::planning::sections::github_slug;
use crate::planning::trace::settle::short_hash;

/// The mode of a ledger this module creates.
const NEW_LEDGER_MODE: u32 = 0o644;
/// The title line of a new ledger.
const LEDGER_TITLE: &str = "# Findings\n";

/// One review item to record.
struct Candidate {
    stage_id: String,
    text: String,
    /// The GitHub anchor of the review section the item sits in.
    section: String,
}

/// Append one entry per item of the review's `## Open Findings` and `## Open Questions` that the
/// feature's ledger does not hold yet, and return how many were added.
///
/// `plan_rel` is the plan's repository-relative path. A standalone plan, or a path from which no
/// feature can be read, imports nothing. An item is already present when an entry carries its
/// `Source:` line, which names the review section, the stage and a hash of the item text, so a
/// second import of the same review adds nothing. A symlinked ledger, or a symlinked directory
/// between it and `root`, is refused and left untouched.
pub fn import_review(root: &Path, plan_rel: &Path, review_text: &str) -> Result<usize> {
    let DocKind::Plan {
        slug,
        feature: Some(feature),
        ..
    } = kinds::classify(plan_rel)
    else {
        return Ok(0);
    };
    let candidates = candidates(review_text);
    if candidates.is_empty() {
        return Ok(0);
    }
    let rel = ledger_path(&feature);
    refuse_symlinks(root, &rel)?;
    let plan_text = read_to_string_bounded(root, plan_rel, MAX_DOC_BYTES)
        .with_context(|| format!("reading {}", plan_rel.display()))?;
    let plan = Plan {
        file: format!("PLAN-{slug}.md"),
        implements: implements_tokens(&plan_text),
    };
    let _lock = lock_plan(&root.join(&rel))?;
    let existing = read_existing(root, &rel)?;
    let (text, added) = append(existing.as_deref(), &plan, &candidates);
    if added == 0 {
        return Ok(0);
    }
    let written = match existing {
        Some(_) => replace_no_follow(root, &rel, text.as_bytes()),
        None => restore_no_follow(root, &rel, text.as_bytes(), NEW_LEDGER_MODE),
    };
    written.with_context(|| format!("writing {}", rel.display()))?;
    Ok(added)
}

/// The plan an import is for.
struct Plan {
    /// `PLAN-<slug>.md`, the name its stages are addressed by.
    file: String,
    /// The tokens of its `Implements:` line, as findings' `About:` names them.
    implements: Vec<String>,
}

/// The ledger text with the new entries appended, and how many there are. `existing` is the
/// ledger as it is, `None` when there is no file yet.
fn append(existing: Option<&str>, plan: &Plan, candidates: &[Candidate]) -> (String, usize) {
    let base = existing.filter(|text| !text.trim().is_empty());
    let ledger = parse(base.unwrap_or_default());
    // Every `Source:` line of the file, so an entry the reader cannot parse still dedupes.
    let mut known: HashSet<String> = base
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.strip_prefix("Source:"))
        .map(|value| value.trim().to_owned())
        .collect();
    let mut number = next_number(&ledger);
    let mut text = base.unwrap_or(LEDGER_TITLE).to_owned();
    if !text.ends_with('\n') {
        text.push('\n');
    }
    let mut added = 0;
    for candidate in candidates {
        let source = source_line(plan, candidate);
        if !known.insert(source.clone()) {
            continue;
        }
        let mut about = vec![format!("{}#stage:{}", plan.file, candidate.stage_id)];
        about.extend(plan.implements.iter().cloned());
        text.push('\n');
        text.push_str(&entry_text(
            &format!("F-{number}"),
            &title(&candidate.text),
            &about,
            Some(&source),
            &FindingStatus::Open,
            &candidate.text,
        ));
        number += 1;
        added += 1;
    }
    (text, added)
}

/// `REVIEW-<plan stem>.md#<section anchor> (<stage id>, <12-hex hash of the item text>)`.
fn source_line(plan: &Plan, candidate: &Candidate) -> String {
    let hash = short_hash(&body_hash(candidate.text.as_bytes()));
    format!(
        "REVIEW-{}#{} ({}, {hash})",
        plan.file, candidate.section, candidate.stage_id
    )
}

/// The items worth recording: findings, then questions, in review order, empty ones dropped.
fn candidates(review_text: &str) -> Vec<Candidate> {
    let mut found = Vec::new();
    for section in [ReviewSection::OpenFindings, ReviewSection::OpenQuestions] {
        let anchor = github_slug(section.heading().trim_start_matches('#').trim());
        for item in review_items(review_text, section) {
            if item.text.trim().is_empty() {
                continue;
            }
            found.push(Candidate {
                stage_id: item.stage_id,
                text: item.text,
                section: anchor.clone(),
            });
        }
    }
    found
}

/// The plan's `Implements:` tokens: anchored ones as written, whole sections as `section_NN`.
fn implements_tokens(plan_text: &str) -> Vec<String> {
    let claims = implements::parse(plan_text).claims();
    claims
        .into_iter()
        .map(|found| match found.anchor {
            Some(anchor) => format!("section_{:02}#{anchor}", found.number),
            None => format!("section_{:02}", found.number),
        })
        .collect()
}

/// Refuse a symlink at any component of `rel` below `root`. A missing component ends the check:
/// nothing below it exists to follow.
fn refuse_symlinks(root: &Path, rel: &Path) -> Result<()> {
    match symlink_component(root, rel)? {
        Some(link) => bail!("refusing to write through the symlink {}", link.display()),
        None => Ok(()),
    }
}

/// The ledger's text, or `None` when the file does not exist.
fn read_existing(root: &Path, rel: &Path) -> Result<Option<String>> {
    match std::fs::symlink_metadata(root.join(rel)) {
        Ok(_) => read_to_string_bounded(root, rel, MAX_DOC_BYTES)
            .map(Some)
            .with_context(|| format!("reading {}", rel.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("checking {}", rel.display())),
    }
}
