//! The command table of one session: the `initialize` rows merged with the native table, each
//! row's source read from its description marker and its class from the native table.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::builtins::{is_native, native_entries};
use crate::plan_session::transcript::cap_chars;

/// Stored-length caps for the untrusted strings of an `initialize` row, in characters; a
/// command name read from a transcript record is capped alike.
pub const NAME_CHARS: usize = 200;
const DESCRIPTION_CHARS: usize = 1_000;
const HINT_CHARS: usize = 200;

/// Description suffixes that name a row's source, each after one space.
const PERSONAL_MARKER: &str = " (user)";
const ACCOUNT_MARKER: &str = " (claude.ai sync)";
/// The bundled `deep-research` row's suffix.
const WORKFLOW_MARKER: &str = " (dynamic workflow)";

/// One command a session's `/` menu lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEntry {
    pub name: String,
    pub aliases: Vec<String>,
    /// The description with its source marker stripped.
    pub description: String,
    pub argument_hint: String,
    pub source: CommandSource,
    pub class: CommandClass,
    /// Why a terminal-only command is not sent from the planner; `None` for the others.
    pub reason: Option<String>,
}

/// Where a command comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandSource {
    /// A native Claude Code command.
    BuiltIn,
    /// A skill bundled with Claude Code.
    Bundled,
    Plugin,
    /// A `~/.claude` skill or command.
    Personal,
    /// A claude.ai account skill.
    Account,
    /// A row with no source marker (a project skill).
    Skill,
}

/// What the planner does with a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommandClass {
    Run,
    /// Sent, with planner handling of its local output (`compact`, `context`).
    RunWithHandling,
    TerminalOnly,
}

/// Whether the session's own command list is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogueState {
    Loading,
    /// The `generation`-th list the side process reported.
    Ready {
        generation: u64,
    },
    Unavailable {
        reason: String,
    },
}

/// A session's commands: the listed rows in `initialize` order, then the native rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTable {
    state: CatalogueState,
    entries: Vec<CommandEntry>,
}

impl CommandTable {
    /// The table before the side process answers: the native rows only.
    pub fn loading() -> Self {
        Self {
            state: CatalogueState::Loading,
            entries: native_entries(),
        }
    }

    /// The table when the session's list cannot be read: the native rows only.
    pub fn unavailable(reason: &str) -> Self {
        Self {
            state: CatalogueState::Unavailable {
                reason: reason.to_owned(),
            },
            entries: native_entries(),
        }
    }

    /// The table of an `initialize` answer's (or a `commands_changed` line's) `commands` rows.
    /// A row named like a native command or alias gives way to the native row, and a listed
    /// alias of that kind is dropped: Claude Code runs the native command, so the native class
    /// must decide it. A row without a string `name`, or repeating an earlier name, is skipped.
    pub fn from_initialize(commands: &[Value], generation: u64) -> Self {
        let mut seen = HashSet::new();
        let mut entries: Vec<CommandEntry> = commands
            .iter()
            .filter_map(|row| {
                let name = row.get("name").and_then(Value::as_str)?;
                let name = cap_chars(name, NAME_CHARS);
                let listed = !name.is_empty() && !is_native(&name) && seen.insert(name.clone());
                listed.then(|| listed_entry(row, name))
            })
            .collect();
        entries.extend(native_entries());
        Self {
            state: CatalogueState::Ready { generation },
            entries,
        }
    }

    pub fn state(&self) -> &CatalogueState {
        &self.state
    }

    pub fn entries(&self) -> &[CommandEntry] {
        &self.entries
    }

    /// The entry `name` (without its slash) names: a name first, then an alias.
    pub fn lookup(&self, name: &str) -> Option<&CommandEntry> {
        let by_name = self.entries.iter().find(|entry| entry.name == name);
        by_name.or_else(|| {
            let mut entries = self.entries.iter();
            entries.find(|entry| entry.aliases.iter().any(|alias| alias == name))
        })
    }
}

/// A non-native row: its source from the description marker, which is stripped; always `Run`.
fn listed_entry(row: &Value, name: String) -> CommandEntry {
    let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or_default();
    let builtin = row.get("builtin") == Some(&Value::Bool(true));
    let (source, description) = source_of(builtin, &name, text("description"));
    let description = description
        .strip_suffix(WORKFLOW_MARKER)
        .unwrap_or(description);
    let aliases = row.get("aliases").and_then(Value::as_array);
    let aliases = aliases.into_iter().flatten().filter_map(Value::as_str);
    let aliases = aliases.map(|alias| cap_chars(alias, NAME_CHARS));
    CommandEntry {
        aliases: aliases.filter(|alias| !is_native(alias)).collect(),
        description: cap_chars(description, DESCRIPTION_CHARS),
        argument_hint: cap_chars(text("argumentHint"), HINT_CHARS),
        source,
        class: CommandClass::Run,
        reason: None,
        name,
    }
}

/// The source the row's flags and description marker name, and the description without it.
fn source_of<'a>(builtin: bool, name: &str, description: &'a str) -> (CommandSource, &'a str) {
    if builtin {
        return (CommandSource::Bundled, description);
    }
    if name.contains(':') {
        if let Some(rest) = without_plugin_prefix(description) {
            return (CommandSource::Plugin, rest);
        }
    }
    if let Some(rest) = description.strip_suffix(PERSONAL_MARKER) {
        return (CommandSource::Personal, rest);
    }
    if let Some(rest) = description.strip_suffix(ACCOUNT_MARKER) {
        return (CommandSource::Account, rest);
    }
    (CommandSource::Skill, description)
}

/// `description` without a leading `(<plugin>)` and the space after it.
fn without_plugin_prefix(description: &str) -> Option<&str> {
    let inner = description.strip_prefix('(')?;
    let close = inner.find(')')?;
    if close == 0 || inner[..close].contains('(') {
        return None;
    }
    let rest = &inner[close + 1..];
    Some(rest.strip_prefix(' ').unwrap_or(rest))
}
