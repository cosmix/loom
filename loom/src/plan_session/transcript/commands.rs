//! Slash command records of Claude Code 2.1.321 (`tests/fixtures/plan_session/probe-notes.md`,
//! "Record shapes per class"): the `<command-name>` record of a prompt-type or a local command,
//! a local command's output, a fork skill's launch and the unknown-name warning. Output becomes
//! an info notice under the command; the mark is what the bridge confirms the delivery with.
//!
//! Record bodies are untrusted: names and args are capped, and a notice is cleaned of terminal
//! escapes and controls before it reaches the chat model.

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::records::{cap_chars, texts, ITEM_CHARS, LINE_CHARS};
use super::{Ingest, Mapper, RecordMark};
use crate::plan_session::commands::NAME_CHARS;
use crate::plan_session::input::sanitize;
use crate::plan_session::model::NoticeKind;
use crate::testrun::recognize::strip_ansi;

const NAME: &str = "command-name";
/// The element a prompt-type record opens with.
const MESSAGE_OPEN: &str = "<command-message>";
const ARGS: &str = "command-args";
/// A local command's output elements, stdout first.
const OUTPUT: [&str; 2] = ["local-command-stdout", "local-command-stderr"];
const FORK: &str = "forked-skill-launch";
const UNKNOWN: &str = "Unknown command: /";

/// What a slash command's record says (names without their slash, element texts trimmed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandMark {
    /// The `<command-name>` record of `/name args`; `prompt` for the prompt-type shape (a user
    /// record of origin `human`), else a local command's.
    Invoked {
        name: String,
        args: String,
        prompt: bool,
    },
    /// A local command's output; `run` is the `(command, args)` its `commandRun` names.
    Output { run: Option<(String, String)> },
    /// `Unknown command: /name`: Claude Code ran no such command.
    Unknown { name: String },
    /// A fork skill's launch: the skill runs on as a background agent.
    Forked { skill: String },
}

/// A command record: its mark, and the notice its output shows.
pub(super) struct CommandRecord {
    pub(super) mark: CommandMark,
    pub(super) notice: Option<String>,
}

/// The command record `record` is; `None` for any other record. A meta record (the caveat, the
/// markdown copy `/context` writes for the model) is none.
pub(super) fn command_record(record: &Value) -> Option<CommandRecord> {
    if record.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    match record.get("type").and_then(Value::as_str)? {
        "user" => {
            let text = texts(record.pointer("/message/content")?).join("\n");
            let human = record.pointer("/origin/kind").and_then(Value::as_str) == Some("human");
            tagged(&text, human, None)
        }
        "system" => {
            let content = record.get("content").and_then(Value::as_str)?;
            // An `error` level record stays an API error notice (`records.rs`).
            let error = record.get("level").and_then(Value::as_str) == Some("error");
            match record.get("subtype").and_then(Value::as_str)? {
                "local_command" => tagged(content, false, command_run(record)),
                "informational" if !error => Some(informational(content)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// The tagged shapes: a fork launch, a `<command-name>` record, or a record that opens with an
/// output element.
fn tagged(text: &str, prompt: bool, run: Option<(String, String)>) -> Option<CommandRecord> {
    if let Some(skill) = element(text, FORK).and_then(fork_skill) {
        let mark = CommandMark::Forked { skill };
        return Some(CommandRecord {
            mark,
            notice: output(text),
        });
    }
    let lead = text.trim_start();
    if lead.starts_with(MESSAGE_OPEN) || lead.starts_with(&format!("<{NAME}>")) {
        let name = element(text, NAME)?;
        let name = name.strip_prefix('/').unwrap_or(name);
        let args = element(text, ARGS).unwrap_or_default();
        let (name, args) = (cap_chars(name, NAME_CHARS), cap_chars(args, LINE_CHARS));
        let mark = CommandMark::Invoked { name, args, prompt };
        return Some(CommandRecord { mark, notice: None });
    }
    let opens = OUTPUT
        .iter()
        .any(|tag| lead.starts_with(&format!("<{tag}>")));
    opens.then(|| CommandRecord {
        mark: CommandMark::Output { run },
        notice: output(text),
    })
}

/// An `informational` record: always a notice; `Unknown command: /x` also names `x`.
fn informational(content: &str) -> CommandRecord {
    let unknown = content.trim().strip_prefix(UNKNOWN);
    let name = unknown.and_then(|rest| rest.split_whitespace().next());
    let mark = match name {
        Some(name) => CommandMark::Unknown {
            name: cap_chars(name, NAME_CHARS),
        },
        None => CommandMark::Output { run: None },
    };
    let notice = clean(content);
    CommandRecord { mark, notice }
}

/// The `commandRun` of a `local_command` output record.
fn command_run(record: &Value) -> Option<(String, String)> {
    let run = record.get("commandRun")?;
    let text = |key: &str| run.get(key).and_then(Value::as_str).map(str::trim);
    let command = text("command")?;
    let command = command.strip_prefix('/').unwrap_or(command);
    let args = text("args").unwrap_or_default();
    Some((cap_chars(command, NAME_CHARS), cap_chars(args, LINE_CHARS)))
}

/// The skill a `<forked-skill-launch>` body names: its `skillName`, else its description's
/// first word.
fn fork_skill(body: &str) -> Option<String> {
    let launch: Value = serde_json::from_str(body).ok()?;
    let text = |key: &str| launch.get(key).and_then(Value::as_str).map(str::trim);
    let named = text("skillName").filter(|name| !name.is_empty());
    let described = || text("description")?.split_whitespace().next();
    let skill = named.or_else(described)?;
    let skill = skill.strip_prefix('/').unwrap_or(skill);
    (!skill.is_empty()).then(|| cap_chars(skill, NAME_CHARS))
}

/// The trimmed text of the first `<tag>` element; an element never closed runs to the end.
fn element<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = text.find(&open)? + open.len();
    let rest = &text[start..];
    let end = rest.find(&format!("</{tag}>")).unwrap_or(rest.len());
    Some(rest[..end].trim())
}

/// The cleaned text of a record's output elements, stdout first; `None` when blank.
fn output(text: &str) -> Option<String> {
    let parts: Vec<&str> = OUTPUT.iter().filter_map(|tag| element(text, tag)).collect();
    clean(&parts.join("\n"))
}

/// `text` as a notice: ANSI escapes (CSI and OSC) and every other control but newline and tab
/// removed, trimmed, capped at [`ITEM_CHARS`] (a `/context` table passes the notice line cap);
/// `None` when nothing is left.
fn clean(text: &str) -> Option<String> {
    let plain = sanitize(&strip_ansi(text));
    let capped = cap_chars(plain.trim(), ITEM_CHARS);
    let trimmed = capped.trim_end();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

impl Mapper {
    /// A command record: its notice in the open turn (else one it opens), then its mark.
    pub(super) fn command(
        &mut self,
        uuid: &str,
        at: Option<DateTime<Utc>>,
        record: CommandRecord,
        out: &mut Ingest,
    ) {
        if let Some(text) = record.notice {
            self.notice(uuid, at, NoticeKind::Info, text, &mut out.deltas);
        }
        out.marks.push(RecordMark::Command(record.mark));
    }
}
