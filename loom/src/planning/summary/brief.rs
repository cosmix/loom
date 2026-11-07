//! The rebuild brief a summary session receives.

use std::fmt::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::claims::{create_claim, delete_claim, draft_path, ClaimHeld};
use super::evaluate::{evaluate, SentenceState, SummaryEval};
use super::format::{sentence_limit, MAX_OPENING_PARAGRAPH};
use super::lock::with_store_lock;
use super::store::{self, validate_id};
use super::write::read_summary;
use crate::context::untrusted::terminal_safe;
use crate::planning::briefs::bundle_files;
use crate::planning::kinds::vision_path;
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
                Err(holder) => bail!(ClaimHeld {
                    summary: id.clone(),
                    holder: holder.session,
                    release: false,
                }),
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
        write_summary_brief(&mut out, project, paths, eval, note)?;
    }
    Ok(out)
}

fn write_summary_brief(
    out: &mut String,
    project: &Project,
    paths: &RuntimePaths,
    eval: &SummaryEval,
    note: Option<&str>,
) -> Result<()> {
    let id = &eval.id;
    let overview = store::is_overview(id);
    write_heading(out, project, eval, overview)?;
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
    write_rules(out, project, id, overview)?;
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
    for path in sources(project, eval) {
        writeln!(out, "- {}", terminal_safe(&path.to_string_lossy()))?;
    }
    Ok(())
}

/// What the summary is built from: a document's source (and a plan's brief bundle), or an
/// overview's vision or metaplans and its member summaries.
fn sources(project: &Project, eval: &SummaryEval) -> Vec<PathBuf> {
    let Some(scope) = store::Scope::of(project, &eval.id) else {
        let briefs = eval
            .source
            .iter()
            .flat_map(|plan| bundle_files(project, plan));
        return eval.source.iter().cloned().chain(briefs).collect();
    };
    if eval.id.ends_with("/product") {
        std::iter::once(vision_path(&scope.feature))
            .chain(scope.product.iter().map(|id| store::summary_path(id)))
            .collect()
    } else {
        project
            .feature(&scope.feature)
            .into_iter()
            .flat_map(|feature| feature.metaplans.iter().map(|(path, _)| path.clone()))
            .chain(scope.engineering.iter().map(|id| store::summary_path(id)))
            .collect()
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

/// What the summary's sentences may cite; a plan's two levels cite different things.
fn cite_rule(id: &str, overview: bool) -> &'static str {
    if overview {
        "cite every member summary and the vision or every metaplan; member citations must point to current sentences"
    } else if store::is_plan(id) {
        "opening sentences cite only this summary's detail sentences, detail sentences cite only the plan and the brief files listed under Sources at valid anchors, and every Markdown brief file is cited at least once (a brief with no Markdown text, such as a JSON mockup, may be read but is not cited)"
    } else {
        "cite only the source document and its valid anchors"
    }
}

fn write_rules(out: &mut String, project: &Project, id: &str, overview: bool) -> Result<()> {
    writeln!(out, "Body rules:")?;
    writeln!(
        out,
        "- Body only, no front matter; start with a # title without links."
    )?;
    writeln!(out, "- Use blank lines, # or ## headings without links, and sentence lines; no lists, tables, fences, indentation, quotes, HTML, or deeper headings.")?;
    writeln!(out, "- Each sentence has nonempty text followed only by citation links at the end; no duplicate sentences.")?;
    writeln!(
        out,
        "- At most {} sentences; {}.",
        sentence_limit(id),
        cite_rule(id, overview)
    )?;
    if store::is_plan(id) {
        writeln!(out, "- A plan summary has two levels. First the opening level: about one sentence per topic, each citing every sentence of the detail paragraph(s) it opens, in order, as [d](#s-xxxxxxxx). Then the detail level: short paragraphs whose sentences cite the plan's heading sections and its brief files, one paragraph per stage citing the stage as PLAN-<slug>.md#stage:<id> and that stage's brief files. No heading after the opening level.")?;
        writeln!(out, "- Write the opening level as paragraphs of one to {MAX_OPENING_PARAGRAPH} sentences that share a topic, a blank line between paragraphs: the Summary page draws each paragraph as a block, and a longer one is refused.")?;
        let caps = store::plan_caps(project, id);
        writeln!(
            out,
            "- At most {} opening and {} detail sentences; the detail level is a real expansion of the opening level, so use the room: cover each stage and each brief file in several sentences, not one.",
            caps.opening, caps.detail
        )?;
        writeln!(out, "- Write the detail level first and seal: the refusal lists each detail sentence's id. Then add the opening level above it and seal again.")?;
    }
    if overview {
        writeln!(out, "- Rewrite a sentence that repeats the sentence it cites to say what the cited sentences add up to, even when it is marked [keep].")?;
        writeln!(out, "- An overview sentence that cites member sentences cites at least two of them and says what they add up to; it may cite one only when that member's summary holds a single citable sentence.")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
