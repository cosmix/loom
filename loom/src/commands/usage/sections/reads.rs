//! Counts both native and shell-based file reads so teams cannot make the
//! report look better merely by changing tools. These findings guide limits
//! on repeated context and on broad documentation retrieval.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::{Entry, ToolUse, Transcript};
use crate::context::untrusted::inline_safe;

use super::fmt::{format_u64, heading, no_data, row};

#[derive(Debug, serde::Serialize)]
pub struct Reads {
    pub total_reads: usize,
    pub top_files: Vec<FileRead>,
    pub same_transcript_rereads: usize,
    pub session_tree_rereads: usize,
    pub unbounded: ReadCount,
    pub knowledge: ReadCount,
}

#[derive(Debug, serde::Serialize)]
pub struct FileRead {
    pub path: String,
    pub reads: usize,
    pub bytes: u64,
}

#[derive(Debug, serde::Serialize, Default)]
pub struct ReadCount {
    pub reads: usize,
    pub bytes: u64,
}

struct Observation {
    transcript: usize,
    session_id: String,
    path: String,
    bytes: u64,
    unbounded: bool,
    knowledge: bool,
}

pub fn build(transcripts: &[Transcript]) -> Reads {
    let observations = observations(transcripts);
    let top_files = top_files(&observations);
    let same_transcript_rereads = repeats_by(&observations, |item| {
        format!("{}\u{0}{}", item.transcript, item.path)
    });
    let session_tree_rereads = repeats_by(&observations, |item| {
        format!("{}\u{0}{}", item.session_id, item.path)
    });
    let unbounded = count_where(&observations, |item| item.unbounded);
    let knowledge = count_where(&observations, |item| item.knowledge);
    Reads {
        total_reads: observations.len(),
        top_files,
        same_transcript_rereads,
        session_tree_rereads,
        unbounded,
        knowledge,
    }
}

pub fn render(reads: &Reads) {
    heading("File reads");
    if reads.total_reads == 0 {
        no_data("file reads");
        return;
    }
    row("reads", format_u64(reads.total_reads as u64));
    row("same-transcript re-reads", reads.same_transcript_rereads);
    row("session-tree re-reads", reads.session_tree_rereads);
    row("unbounded results over 20,000 bytes", reads.unbounded.reads);
    row("unbounded result bytes", format_u64(reads.unbounded.bytes));
    row("knowledge-path reads", reads.knowledge.reads);
    row("knowledge-path bytes", format_u64(reads.knowledge.bytes));
    println!("  top files by returned bytes:");
    for file in &reads.top_files {
        println!(
            "    {}: {} bytes ({} reads)",
            inline_safe(&file.path),
            format_u64(file.bytes),
            file.reads
        );
    }
}

fn observations(transcripts: &[Transcript]) -> Vec<Observation> {
    let mut results = Vec::new();
    for (index, transcript) in transcripts.iter().enumerate() {
        let answer_bytes = result_bytes(transcript);
        for request in transcript.requests() {
            for tool in &request.tool_uses {
                if let Some(path) = read_path(tool) {
                    let bytes = answer_bytes.get(&tool.id).copied().unwrap_or(0);
                    results.push(Observation {
                        transcript: index,
                        session_id: transcript.session_id.clone(),
                        unbounded: bytes > 20_000 && tool.input.get("limit").is_none(),
                        knowledge: is_knowledge_path(&path),
                        path,
                        bytes,
                    });
                }
            }
        }
    }
    results
}

fn result_bytes(transcript: &Transcript) -> BTreeMap<String, u64> {
    let mut answers = BTreeMap::new();
    for entry in &transcript.entries {
        if let Entry::User(user) = entry {
            if let Some(id) = &user.tool_use_id {
                answers.insert(id.clone(), user.text.len() as u64);
            }
        }
    }
    answers
}

fn read_path(tool: &ToolUse) -> Option<String> {
    if tool.name == "Read" {
        return tool.input.get("file_path")?.as_str().map(str::to_owned);
    }
    if tool.name != "Bash" {
        return None;
    }
    let command = tool.input.get("command")?.as_str()?.trim_start();
    if !is_bash_read(command) {
        return None;
    }
    let tokens = command.split_whitespace().collect::<Vec<_>>();
    let end = tokens
        .iter()
        .position(|part| matches!(*part, "|" | ">" | "<"))
        .unwrap_or(tokens.len());
    tokens[..end]
        .iter()
        .rfind(|part| !part.starts_with('-'))
        .copied()
        .map(clean_path)
}

fn is_bash_read(command: &str) -> bool {
    command == "cat"
        || command.starts_with("cat ")
        || command.starts_with("sed -n")
        || command == "head"
        || command.starts_with("head ")
        || command == "tail"
        || command.starts_with("tail ")
}

fn clean_path(raw: &str) -> String {
    raw.trim_matches(|character| character == '\'' || character == '"')
        .to_owned()
}

fn top_files(observations: &[Observation]) -> Vec<FileRead> {
    let mut grouped = BTreeMap::<String, FileRead>::new();
    for item in observations {
        let file = grouped
            .entry(item.path.clone())
            .or_insert_with(|| FileRead {
                path: item.path.clone(),
                reads: 0,
                bytes: 0,
            });
        file.reads += 1;
        file.bytes += item.bytes;
    }
    let mut files = grouped.into_values().collect::<Vec<_>>();
    files.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.path.cmp(&right.path))
    });
    files.truncate(10);
    files
}

fn repeats_by(observations: &[Observation], key: impl Fn(&Observation) -> String) -> usize {
    let mut counts = BTreeMap::<String, usize>::new();
    for item in observations {
        *counts.entry(key(item)).or_default() += 1;
    }
    counts
        .into_values()
        .map(|count| count.saturating_sub(1))
        .sum()
}

fn count_where(observations: &[Observation], matches: impl Fn(&Observation) -> bool) -> ReadCount {
    let mut result = ReadCount::default();
    for item in observations {
        if matches(item) {
            result.reads += 1;
            result.bytes += item.bytes;
        }
    }
    result
}

fn is_knowledge_path(path: &str) -> bool {
    path.starts_with("doc/loom/knowledge/") || path.contains("/doc/loom/knowledge/")
}
