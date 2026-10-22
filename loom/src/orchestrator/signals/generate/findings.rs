//! The `## Dependency Findings` section: relevant, unsettled journal entries of
//! each accepted direct dependency, one line each, under a hard byte cap.

use std::collections::HashSet;
use std::path::Path;

use glob::{MatchOptions, Pattern};

use crate::context::rank_source::{join_working_dir, normalize_dependency_path};
use crate::context::render::render_excerpt_block;
use crate::context::schema::ContextPack;
use crate::fs::memory::{
    list_journals, read_journal, read_staged_entries, settled_ids, MemoryEntry, MemoryEntryType,
};
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::orchestrator::signals::format::REFERENCE_DATA_SENTENCE;
use crate::orchestrator::signals::retrieval::{dependency_changes, DependencyChange};
use crate::utils::truncate_for_display;
use crate::verify::review::report::single_line;

/// The section's `## ` heading, also listed in `parse.rs::EXPECTED_SECTIONS`.
pub(crate) const DEPENDENCY_FINDINGS_HEADING: &str = "Dependency Findings";
/// The whole section, heading through closing fence, never exceeds this.
pub(crate) const MAX_DEPENDENCY_FINDINGS_BYTES: usize = 1600;

/// Characters kept of an entry's claim, and of its rationale and evidence.
const CLAIM_CHARS: usize = 200;
const DETAIL_CHARS: usize = 100;
/// Note prefixes that rank together (`NotePrefix` stays under `commands/`).
const RANKED_NOTE_PREFIXES: [&str; 4] = ["mistake:", "gotcha:", "found:", "found/gotcha:"];

/// An accepted dependency and its whole journal, receipts included.
struct Producer {
    change: DependencyChange,
    entries: Vec<MemoryEntry>,
}

/// What a recipient matches entries against: declared files and artifacts as
/// repository-relative globs, and brief paths. Integration-verify takes every
/// entry; on plan v2 minus suggestions, which its Reviewer Suggestions list.
struct Recipient {
    id: String,
    takes_all: bool,
    drops_suggestions: bool,
    patterns: Vec<Pattern>,
    brief_paths: HashSet<String>,
}

impl Recipient {
    fn new(stage: &Stage, pack: Option<&ContextPack>) -> Self {
        let takes_all = stage.stage_type == StageType::IntegrationVerify;
        let patterns = stage
            .files
            .iter()
            .chain(&stage.artifacts)
            .filter_map(|entry| join_working_dir(stage.working_dir.as_deref(), entry))
            .filter_map(|path| Pattern::new(&path).ok())
            .collect();
        let items = pack.into_iter().flat_map(|pack| &pack.items);
        let brief_paths = items
            .map(|item| normalize_dependency_path(&item.pointer.path.to_string_lossy()))
            .collect();
        Self {
            id: stage.id.clone(),
            takes_all,
            drops_suggestions: takes_all && stage.plan_version == 2,
            patterns,
            brief_paths,
        }
    }

    fn matches(&self, path: &str) -> bool {
        let options = MatchOptions {
            require_literal_separator: true,
            ..MatchOptions::new()
        };
        self.brief_paths.contains(path)
            || self.patterns.iter().any(|p| p.matches_with(path, options))
    }
}

/// A kept entry and the index of the producer whose journal holds it.
type Selected<'a> = (usize, &'a MemoryEntry);

/// `None` for knowledge-distill (its prefix reads every journal), without an
/// accepted non-knowledge direct dependency, or on an unreadable journal. The
/// git calls `retrieve_stage_pack` made repeat: sharing them costs a signature.
pub(crate) fn build_dependency_findings(
    work_dir: &Path,
    stage: &Stage,
    pack: Option<&ContextPack>,
) -> Option<String> {
    if stage.stage_type == StageType::KnowledgeDistill || stage.dependencies.is_empty() {
        return None;
    }
    let changes: Vec<DependencyChange> = dependency_changes(work_dir, stage)
        .into_iter()
        .filter(|change| {
            change.stage.status == StageStatus::Completed
                && change.stage.stage_type != StageType::Knowledge
        })
        .collect();
    if changes.is_empty() {
        return None;
    }
    let staged = list_journals(work_dir)
        .and_then(|journals| read_staged_entries(work_dir, journals, read_journal));
    let entries = match staged {
        Ok((entries, _)) => entries,
        Err(error) => {
            tracing::debug!(%error, stage_id = %stage.id, "dependency findings skipped");
            return None;
        }
    };
    let settled = settled_ids(entries.iter().map(|(_, entry)| entry));
    let producers = producers(changes, &entries);
    let root = WorkDir::new(work_dir)
        .ok()
        .and_then(|dir| dir.main_project_root());
    let root = root.as_deref().and_then(Path::to_str);
    let selected = select(&Recipient::new(stage, pack), &producers, &settled, root);
    Some(render(&producers, &selected))
}

/// Pair each change with its own journal's entries: an entry's producer is
/// the journal holding it, never its `session` field.
fn producers(changes: Vec<DependencyChange>, all: &[(String, MemoryEntry)]) -> Vec<Producer> {
    let own = |change: DependencyChange| {
        let mine = all.iter().filter(|(j, _)| *j == change.stage.id);
        let entries = mine.map(|(_, entry)| entry.clone()).collect();
        Producer { change, entries }
    };
    changes.into_iter().map(own).collect()
}

/// Relevant, non-excluded entries by rank, dependency position, timestamp,
/// then id (timestamps have one-second granularity).
fn select<'a>(
    recipient: &Recipient,
    producers: &'a [Producer],
    settled: &HashSet<String>,
    root: Option<&str>,
) -> Vec<Selected<'a>> {
    let mut kept = Vec::new();
    for (dep, producer) in producers.iter().enumerate() {
        let producer_dir = producer.change.stage.working_dir.as_deref();
        for entry in &producer.entries {
            if is_excluded(entry, settled, recipient.drops_suggestions) {
                continue;
            }
            let names_recipient = names_stage(entry, &recipient.id);
            let relevant = recipient.takes_all
                || names_recipient
                || path_candidates(entry, producer_dir, root)
                    .iter()
                    .any(|path| recipient.matches(path));
            if relevant {
                let order = rank(entry, names_recipient);
                kept.push(((order, dep, entry.timestamp, entry.id.as_str()), entry));
            }
        }
    }
    kept.sort_by_key(|(key, _)| *key);
    kept.into_iter()
        .map(|((_, dep, _, _), entry)| (dep, entry))
        .collect()
}

/// Receipts, changes and `mutation:` notes never reach a successor; a settled
/// entry does not either, except a decision, which still constrains the code.
fn is_excluded(entry: &MemoryEntry, settled: &HashSet<String>, drops: bool) -> bool {
    match entry.entry_type {
        MemoryEntryType::Receipt | MemoryEntryType::Change => true,
        MemoryEntryType::Note if starts_with_ignore_case(&entry.content, "mutation:") => true,
        MemoryEntryType::Suggestion if drops => true,
        MemoryEntryType::Decision => false,
        _ => settled.contains(&entry.id),
    }
}

/// 0 names the recipient, 1 a mistake/gotcha/found note, 2 a decision with a
/// rationale, 3 the rest.
fn rank(entry: &MemoryEntry, names_recipient: bool) -> u8 {
    let is = |kind: MemoryEntryType| entry.entry_type == kind;
    let prefixed = |p: &&str| starts_with_ignore_case(&entry.content, p);
    let rationale = entry.context.as_deref().unwrap_or("");
    if names_recipient {
        0
    } else if is(MemoryEntryType::Note) && RANKED_NOTE_PREFIXES.iter().any(prefixed) {
        1
    } else if is(MemoryEntryType::Decision) && !rationale.trim().is_empty() {
        2
    } else {
        3
    }
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    let head = text.trim_start().get(..prefix.len());
    head.is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

/// Content or context holds `stage_id` as a token split on `[^A-Za-z0-9_-]`.
fn names_stage(entry: &MemoryEntry, stage_id: &str) -> bool {
    let separator = |c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-');
    let context = entry.context.as_deref().unwrap_or("");
    let texts = [entry.content.as_str(), context];
    let mut tokens = texts.into_iter().flat_map(|text| text.split(separator));
    !stage_id.is_empty() && tokens.any(|token| token == stage_id)
}

/// Repository-relative readings of the entry's whitespace-free evidence items
/// and of the slashed tokens of its content.
fn path_candidates(
    entry: &MemoryEntry,
    producer_dir: Option<&str>,
    root: Option<&str>,
) -> Vec<String> {
    let evidence = entry.evidence.iter().map(String::as_str);
    let evidence = evidence.filter(|item| !item.contains(char::is_whitespace));
    let tokens = entry.content.split_whitespace().map(trim_token);
    let tokens = tokens.filter(|token| token.contains('/'));
    let mut candidates: Vec<String> = Vec::new();
    for raw in evidence.chain(tokens) {
        for reading in readings(strip_line_span(raw), producer_dir, root) {
            if !candidates.contains(&reading) {
                candidates.push(reading);
            }
        }
    }
    candidates
}

/// A token without wrapping quotes, brackets, punctuation or a trailing `.`/`:`.
fn trim_token(token: &str) -> &str {
    let wrapper = |c: char| "`'\"()[]{}<>,;".contains(c);
    let mut current = token;
    loop {
        let next = current.trim_matches(wrapper);
        let next = next.trim_end_matches(['.', ':']);
        if next == current {
            return next;
        }
        current = next;
    }
}

/// `value` without a trailing `:<digits>` or `:<digits>-<digits>`.
fn strip_line_span(value: &str) -> &str {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let Some((path, span)) = value.rsplit_once(':') else {
        return value;
    };
    let (start, end) = span.split_once('-').unwrap_or((span, "0"));
    if digits(start) && digits(end) {
        path
    } else {
        value
    }
}

/// A relative value read as repository-relative and against the producer's
/// `working_dir`: journals mix both, so one reading alone misses `src/x.rs`.
fn readings(value: &str, producer_dir: Option<&str>, root: Option<&str>) -> Vec<String> {
    if value.starts_with('/') {
        return repository_relative(value, root).into_iter().collect();
    }
    let both = [
        join_working_dir(None, value),
        join_working_dir(producer_dir, value),
    ];
    both.into_iter().flatten().collect()
}

/// An absolute path loses everything through the stage-id component after
/// `.worktrees/`, or else `root`; any other absolute path names nothing here.
fn repository_relative(value: &str, root: Option<&str>) -> Option<String> {
    if let Some((_, inside)) = value.split_once("/.worktrees/") {
        let (_, path) = inside.split_once('/')?;
        return join_working_dir(None, path);
    }
    let root = root?.trim_end_matches('/');
    let path = value.strip_prefix(root)?.strip_prefix('/')?;
    join_working_dir(None, path)
}

/// Headlines in order while they fit, then each entry that still fits, with the
/// longest footer reserved, so the section stays within the byte cap.
fn render(producers: &[Producer], selected: &[Selected<'_>]) -> String {
    let lines: Vec<(usize, String)> = selected
        .iter()
        .map(|(dep, entry)| (*dep, entry_line(entry)))
        .collect();
    let reserved = footer(producers.len(), lines.len());
    let fits = |deps: usize, shown: &[bool]| {
        section(producers, &lines, deps, shown, &reserved).len() <= MAX_DEPENDENCY_FINDINGS_BYTES
    };
    let mut shown = vec![false; lines.len()];
    let deps = (1..=producers.len())
        .take_while(|&deps| fits(deps, &shown))
        .last()
        .unwrap_or(0);
    for (index, (dep, _)) in lines.iter().enumerate() {
        if *dep < deps {
            shown[index] = true;
            shown[index] = fits(deps, &shown);
        }
    }
    let skipped = shown.iter().filter(|on| !**on).count();
    let actual = footer(producers.len() - deps, skipped);
    section(producers, &lines, deps, &shown, &actual)
}

/// Empty when nothing was skipped.
fn footer(dependencies: usize, entries: usize) -> String {
    let skipped = format!("{entries} more relevant entries omitted for size.");
    match (dependencies, entries) {
        (0, 0) => String::new(),
        (0, _) => skipped,
        _ => format!("{dependencies} more dependencies and {skipped}"),
    }
}

/// The first `deps` headlines, each with its `shown` lines, then `footer`.
fn section(
    producers: &[Producer],
    lines: &[(usize, String)],
    deps: usize,
    shown: &[bool],
    footer: &str,
) -> String {
    let own = move |dep: usize| {
        lines
            .iter()
            .zip(shown)
            .filter(move |((d, _), on)| *d == dep && **on)
            .map(|((_, line), _)| line.as_str())
    };
    let mut body: Vec<String> = Vec::new();
    for (dep, producer) in producers.iter().enumerate().take(deps) {
        body.push(headline(producer, own(dep).count()));
        body.extend(own(dep).map(str::to_string));
    }
    body.extend((!footer.is_empty()).then(|| footer.to_string()));
    let block = render_excerpt_block(&body.join("\n"));
    format!("## {DEPENDENCY_FINDINGS_HEADING}\n\n{REFERENCE_DATA_SENTENCE}\n\n{block}")
}

/// `<id> accepted at <commit8>[ (assumed)]: <shown> of <total> entries[ | git
/// diff --stat <base8> <commit8>] | loom memory list --stage <id>`.
fn headline(producer: &Producer, shown: usize) -> String {
    let DependencyChange { stage, range, .. } = &producer.change;
    let (id, total) = (&stage.id, producer.entries.len());
    let assumed = if stage.merge_assumed {
        " (assumed)"
    } else {
        ""
    };
    let at = match &stage.completed_commit {
        Some(commit) => format!(" at {}{assumed}", prefix8(commit)),
        None => String::new(),
    };
    let mut line = format!("{id} accepted{at}: {shown} of {total} entries");
    if let (Some((base, _)), Some(commit)) = (range, &stage.completed_commit) {
        let (base8, commit8) = (prefix8(base), prefix8(commit));
        line.push_str(&format!(" | git diff --stat {base8} {commit8}"));
    }
    line.push_str(&format!(" | loom memory list --stage {id}"));
    line
}

/// `- [<type> <id8>] <claim>[ | why: <rationale>][ | <evidence>]`, all on one line.
fn entry_line(entry: &MemoryEntry) -> String {
    let claim = truncate_for_display(&single_line(&entry.content), CLAIM_CHARS);
    let id8 = single_line(&prefix8(&entry.id));
    let mut line = format!("- [{} {id8}] {claim}", entry.entry_type);
    let rationale = single_line(entry.context.as_deref().unwrap_or(""));
    let evidence = single_line(&entry.evidence.join(","));
    for (label, text) in [(" | why: ", rationale), (" | ", evidence)] {
        if !text.is_empty() {
            line.push_str(label);
            line.push_str(&truncate_for_display(&text, DETAIL_CHARS));
        }
    }
    line
}

fn prefix8(value: &str) -> String {
    value.chars().take(8).collect()
}

#[cfg(test)]
#[path = "findings_tests.rs"]
mod tests;
