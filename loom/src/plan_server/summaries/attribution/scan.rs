//! Reading Claude Code transcripts for `loom plan summary` calls: the ids a subagent briefed and
//! the last failed seal of a session.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use serde_json::Value;

use crate::context::untrusted::inline_safe;
use crate::planning::summary::store::validate_id;
use crate::tokens::tail::{self, Line};

/// Longest seal error kept, in characters.
const MAX_ERROR_CHARS: usize = 200;
/// What makes a command more than one plain `loom` call (a newline starts a second command).
const COMPOUND: [&str; 6] = [";", "&&", "|", "`", "$(", "\n"];

/// What a transcript's assistant records hold.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Scan {
    /// The ids its Bash calls briefed, in order, each once.
    pub(super) briefed: Vec<String>,
    /// The model of its last assistant record.
    pub(super) model: Option<String>,
}

/// Scan a transcript; one that cannot be read holds nothing.
pub(super) fn scan(path: &Path) -> Scan {
    let mut found = Scan::default();
    for_each_record(path, |record| {
        if record["type"] != "assistant" {
            return;
        }
        let model = record["message"]["model"].as_str();
        if let Some(model) = model.filter(|model| !model.starts_with('<')) {
            found.model = Some(model.to_owned());
        }
        for (_, command) in bash_calls(record) {
            for id in brief_ids(command).unwrap_or_default() {
                if !found.briefed.contains(&id) {
                    found.briefed.push(id);
                }
            }
        }
    });
    found
}

/// The first line of the result of the last `loom plan summary seal` call that failed (an
/// error result, or output naming a `drafts/` path followed by `: `), terminal-safe and at most
/// 200 characters.
pub fn last_seal_error(main_transcript: &Path) -> Option<String> {
    let mut seals: HashSet<String> = HashSet::new();
    let mut last: Option<String> = None;
    for_each_record(main_transcript, |record| match record["type"].as_str() {
        Some("assistant") => {
            let calls = bash_calls(record).filter(|(_, command)| is_seal(command));
            seals.extend(calls.map(|(id, _)| id.to_owned()));
        }
        Some("user") => {
            for (id, is_error, text) in tool_results(record) {
                if seals.contains(id) && (is_error || names_a_draft_error(&text)) {
                    last = Some(text);
                }
            }
        }
        _ => {}
    });
    let text = last?;
    let first = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(inline_safe(first).chars().take(MAX_ERROR_CHARS).collect())
}

/// The ids of a plain `loom plan summary brief <ids...> [--note ...]` call; `None` for any other
/// command, a compound one, or an invalid id.
pub(super) fn brief_ids(command: &str) -> Option<Vec<String>> {
    let rest = summary_call(command, "brief")?;
    let ids: Vec<String> = rest
        .into_iter()
        .take_while(|word| !word.starts_with("--"))
        .map(str::to_owned)
        .collect();
    let valid = !ids.is_empty() && ids.iter().all(|id| validate_id(id).is_ok());
    valid.then_some(ids)
}

fn is_seal(command: &str) -> bool {
    summary_call(command, "seal").is_some()
}

/// The words after `loom plan summary <verb>` of a plain call to the `loom` binary.
fn summary_call<'a>(command: &'a str, verb: &str) -> Option<Vec<&'a str>> {
    if COMPOUND.iter().any(|token| command.contains(token)) {
        return None;
    }
    let mut words = command.split_whitespace();
    let program = words.next()?;
    let loom = Path::new(program)
        .file_name()
        .is_some_and(|name| name == "loom");
    let call: Vec<&str> = words.by_ref().take(3).collect();
    (loom && call == ["plan", "summary", verb]).then(|| words.collect())
}

fn names_a_draft_error(text: &str) -> bool {
    text.find("drafts/")
        .is_some_and(|at| text[at..].contains(": "))
}

/// `(tool_use id, command)` of every Bash `tool_use` block of an assistant record.
fn bash_calls(record: &Value) -> impl Iterator<Item = (&str, &str)> {
    let blocks = record["message"]["content"]
        .as_array()
        .into_iter()
        .flatten();
    let bash = blocks.filter(|block| block["type"] == "tool_use" && block["name"] == "Bash");
    bash.filter_map(|block| Some((block["id"].as_str()?, block["input"]["command"].as_str()?)))
}

/// `(tool_use id, is_error, text)` of every `tool_result` block of a user record.
fn tool_results(record: &Value) -> impl Iterator<Item = (&str, bool, String)> {
    let blocks = record["message"]["content"]
        .as_array()
        .into_iter()
        .flatten();
    let results = blocks.filter(|block| block["type"] == "tool_result");
    results.filter_map(|block| {
        let id = block["tool_use_id"].as_str()?;
        let is_error = block["is_error"].as_bool().unwrap_or(false);
        Some((id, is_error, result_text(&block["content"])))
    })
}

/// A result's content: a string, or the text of its text blocks.
fn result_text(content: &Value) -> String {
    if let Some(text) = content.as_str() {
        return text.to_owned();
    }
    let blocks = content.as_array().into_iter().flatten();
    let texts = blocks.filter(|block| block["type"] == "text");
    let texts: Vec<&str> = texts.filter_map(|block| block["text"].as_str()).collect();
    texts.join("\n")
}

/// Call `f` with every JSON record of the transcript. The read is the token tracker's:
/// no-follow and non-blocking, so a symlink or FIFO in the transcript's place is not opened.
/// Unfinished, over-long and unparsable lines are skipped, and reading stops at the first I/O
/// error.
fn for_each_record(path: &Path, mut f: impl FnMut(&Value)) {
    let Some(stat) = tail::stat(path) else {
        return;
    };
    let stop = AtomicBool::new(false);
    let _ = tail::read_lines(path, &mut 0, stat.len, &stop, |line| {
        if let Line::Text(text) = line {
            if let Ok(record) = serde_json::from_str::<Value>(text) {
                f(&record);
            }
        }
    });
}
