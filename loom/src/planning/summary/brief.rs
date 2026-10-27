//! The rebuild brief a summary session receives.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::claims::{create_claim, delete_claim, draft_path};
use super::evaluate::{evaluate, Evaluation, SentenceState, SummaryEval};
use super::format::{MAX_DOCUMENT_SENTENCES, MAX_OVERVIEW_SENTENCES};
use super::lock::with_store_lock;
use super::store::{self, validate_id};
use super::write::read_summary;
use crate::context::untrusted::{inline_safe, terminal_safe};
use crate::planning::kinds::VISION_PATH;
use crate::planning::project::Project;
use crate::planning::runtime::RuntimePaths;

/// Claim the requested summaries and produce instructions for rebuilding them.
pub fn brief(
    root: &Path,
    paths: &RuntimePaths,
    ids: &[String],
    note: Option<&str>,
) -> Result<String> {
    let mut unique: Vec<String> = Vec::new();
    for id in ids {
        validate_id(id)?;
        if !unique.contains(id) {
            unique.push(id.clone());
        }
    }
    let ids = unique.as_slice();
    let project = Project::load(root)?;
    let expected = store::expected_ids(&project);
    for id in ids {
        if !expected.contains(id) {
            bail!("no such summary in this project: {id}");
        }
    }
    let mut made = Vec::new();
    let result = (|| {
        for id in ids {
            match create_claim(paths, id)? {
                Ok(_) => made.push(id.clone()),
                Err(holder) => bail!(
                    "{id} is claimed by {}",
                    inline_safe(holder.session.as_deref().unwrap_or("a terminal"))
                ),
            }
        }
        with_store_lock(paths, || build_brief(&project, paths, ids, note))
    })();
    if result.is_err() {
        // Best effort: a failed undo must not mask the original error.
        for id in &made {
            let _ = delete_claim(paths, id);
        }
    }
    result
}

fn build_brief(
    project: &Project,
    paths: &RuntimePaths,
    ids: &[String],
    note: Option<&str>,
) -> Result<String> {
    let evaluation = evaluate(project, &[]);
    let mut out = String::new();
    for id in ids {
        let eval = evaluation
            .summaries
            .get(id)
            .context("summary disappeared from evaluation")?;
        write_summary_brief(&mut out, project, paths, &evaluation, eval, note)?;
    }
    Ok(out)
}

fn write_summary_brief(
    out: &mut String,
    project: &Project,
    paths: &RuntimePaths,
    evaluation: &Evaluation,
    eval: &SummaryEval,
    note: Option<&str>,
) -> Result<()> {
    let id = &eval.id;
    let overview = id == "product" || id == "engineering";
    write_heading(out, project, evaluation, eval, overview)?;
    let matched = note.and_then(|n| {
        eval.sentences.iter().find(|s| {
            !n.is_empty()
                && (n.contains(&s.text)
                    || n.contains(&s.id)
                    || s.text.contains(n)
                    || s.id.contains(n))
        })
    });
    if matched.is_none() {
        if let Some(note) = note {
            writeln!(out, "Note: {}", one_line(note))?;
        }
    }
    writeln!(out, "Current summary:")?;
    if eval.file.is_some() {
        if let Some(text) = read_summary(&project.root, id)? {
            print_current(out, &text, eval, matched.map(|s| s.id.as_str()), note)?;
        }
    } else {
        writeln!(out, "(none)")?;
    }
    write_rules(out, overview)?;
    let draft = draft_path(paths, id);
    let absolute = if draft.is_absolute() {
        draft
    } else {
        std::env::current_dir()?.join(draft)
    };
    writeln!(out, "Draft: {}", terminal_safe(&absolute.to_string_lossy()))?;
    writeln!(out, "Seal: loom plan summary seal {id}\n")?;
    Ok(())
}

fn write_heading(
    out: &mut String,
    project: &Project,
    evaluation: &Evaluation,
    eval: &SummaryEval,
    overview: bool,
) -> Result<()> {
    writeln!(
        out,
        "# {} ({})",
        eval.id,
        if overview { "overview" } else { "document" }
    )?;
    writeln!(out, "Sources:")?;
    for path in sources(project, evaluation, eval) {
        writeln!(out, "- {}", terminal_safe(&path.to_string_lossy()))?;
    }
    Ok(())
}

fn sources(project: &Project, evaluation: &Evaluation, eval: &SummaryEval) -> Vec<PathBuf> {
    match eval.id.as_str() {
        "product" => std::iter::once(PathBuf::from(VISION_PATH))
            .chain(
                evaluation
                    .scope
                    .product
                    .iter()
                    .map(|id| store::summary_path(id)),
            )
            .collect(),
        "engineering" => project
            .metaplans
            .iter()
            .map(|(path, _)| path.clone())
            .chain(
                evaluation
                    .scope
                    .engineering
                    .iter()
                    .map(|id| store::summary_path(id)),
            )
            .collect(),
        _ => eval.source.iter().cloned().collect(),
    }
}

fn print_current(
    out: &mut String,
    text: &str,
    eval: &SummaryEval,
    matched: Option<&str>,
    note: Option<&str>,
) -> Result<()> {
    let body_start = text
        .lines()
        .enumerate()
        .skip(1)
        .find(|(_, line)| line.trim_end() == "---")
        .map(|(i, _)| i + 1)
        .unwrap_or(0);
    for (index, line) in text.lines().enumerate().skip(body_start) {
        let sentence = eval.sentences.iter().find(|s| s.line == index + 1);
        let prefix = match sentence.map(|s| s.state) {
            Some(SentenceState::Stale) => "[stale] ",
            Some(SentenceState::Current) => "[keep] ",
            None => "",
        };
        writeln!(out, "{prefix}{}", terminal_safe(line))?;
        if sentence.is_some_and(|s| Some(s.id.as_str()) == matched) {
            if let Some(note) = note {
                writeln!(out, "Note: {}", one_line(note))?;
            }
        }
    }
    Ok(())
}

/// Neutralize `text` for the terminal and keep it on one line.
fn one_line(text: &str) -> String {
    terminal_safe(text).replace(['\n', '\t'], " ")
}

fn write_rules(out: &mut String, overview: bool) -> Result<()> {
    writeln!(out, "Body rules:")?;
    writeln!(
        out,
        "- Body only, no front matter; start with a # title without links."
    )?;
    writeln!(out, "- Use blank lines, # or ## headings without links, and sentence lines; no lists, tables, fences, indentation, quotes, HTML, or deeper headings.")?;
    writeln!(out, "- Each sentence has nonempty text followed only by citation links at the end; no duplicate sentences.")?;
    writeln!(
        out,
        "- At most {} sentences; cite {}.",
        if overview {
            MAX_OVERVIEW_SENTENCES
        } else {
            MAX_DOCUMENT_SENTENCES
        },
        if overview {
            "every member summary and the vision or every metaplan; member citations must point to current sentences"
        } else {
            "only the source document and its valid anchors"
        }
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
