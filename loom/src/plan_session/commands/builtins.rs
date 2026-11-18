//! The native commands of Claude Code 2.1.321 and what the planner does with each.
//!
//! Source: Claude Code 2.1.321, `tests/fixtures/plan_session/probe-notes.md` "Built-in rows".
//! The first 36 rows are the native rows of the headless `initialize` list, their descriptions,
//! aliases and argument hints copied from it; the other 21 are native commands the headless list
//! lacks (seen in the pane or the bundle), described in plain words. A native command a later
//! version adds is a `builtin: true` row this table does not name, so it reads as a bundled skill
//! until the table learns it (the plan's "Parity limit").

use super::list::{CommandClass, CommandEntry, CommandSource};

/// Why the planner leaves a native command to the terminal, or that it handles it.
#[derive(Clone, Copy)]
enum Handling {
    /// The planner runs it and reads its local output (`compact`, `context`).
    Handled,
    /// Ends, replaces or moves the session.
    Changes,
    /// Opens a dialog that never closes by itself.
    Dialog,
    /// Writes the user's default.
    SavesDefault,
    /// Never observed in a planner session.
    Unverified,
}

/// One native row: name, aliases, description, argument hint, handling.
type Native = (
    &'static str,
    &'static [&'static str],
    &'static str,
    &'static str,
    Handling,
);

use Handling::{Changes, Dialog, Handled, SavesDefault, Unverified};

// One row per line, as the probe notes list them; rustfmt would spread each over seven.
#[rustfmt::skip]
const NATIVES: [Native; 57] = [
    ("advisor", &[], "Let Claude consult a stronger model at key moments", "[fable|opus|sonnet|off]", Unverified),
    ("agents", &[], "(removed) Ask Claude to create/manage subagents, or edit .claude/agents/", "", Dialog),
    ("auto-mode-setup", &[], "Teach auto mode about your environment, plus optional rule tweaks", "[--request-id <uuid>] (--wizard posture=… scope=… depth=… --propose | --expect-sha256 <64-hex> --apply-file <path>)", Unverified),
    ("autocompact", &[], "Configure the auto-compact window size", "[auto|<tokens>]", Unverified),
    ("clear", &["reset", "new"], "Start a new session with empty context; previous session stays on disk (resumable with /resume)", "[name]", Changes),
    ("color", &[], "Set the prompt bar color for this session", "[red|blue|green|yellow|purple|orange|pink|cyan|default]", Unverified),
    ("compact", &[], "Free up context by summarizing the conversation so far", "<optional custom summarization instructions>", Handled),
    ("config", &["settings"], "Set a setting by key", "key=value", Dialog),
    ("output-style", &[], "List output styles or switch to one", "[style]", Unverified),
    ("context", &[], "Show current context usage", "", Handled),
    ("effort", &[], "Set effort level for model usage", "<low|medium|high|xhigh|max|auto|ultracode [on|off]>", SavesDefault),
    ("fast", &[], "Toggle fast mode (Opus 5.5)", "[on|off]", Unverified),
    ("focus", &[], "Toggle focus view: just your prompt, summary, and response", "[on|off]", Unverified),
    ("heapdump", &[], "Dump the JS heap to the Desktop; on Linux with no Desktop folder, the home directory", "", Unverified),
    ("init", &[], "Initialize a new CLAUDE.md file with codebase documentation", "", Unverified),
    ("mcp", &[], "Manage MCP servers", "[reconnect|enable|disable [<server>|all]]", Dialog),
    ("import", &[], "Import config from another AI coding agent", "", Unverified),
    ("model", &[], "Set the AI model for Claude Code", "<model>", Dialog),
    ("__remote-workflow", &[], "Run the workflow script delivered in this session environment (server-launched sessions only)", "", Unverified),
    ("workflow-launch-exec", &[], "Execute a server-launched workflow handoff (workflow_launch event sessions only)", "", Unverified),
    ("reload-plugins", &[], "Activate pending plugin changes in the current session", "[--force]", Unverified),
    ("reload-skills", &[], "Pick up skills added or changed on disk during this session", "", Unverified),
    ("rename", &["name"], "Rename the current conversation", "[name]", Changes),
    ("ultrareview", &[], "Start a cloud agent that finds and verifies bugs in your branch (~5-15 min, $5-$25 USD) · Runs in a cloud session. See https://code.claude.com/docs/en/claude-code-on-the-web", "", Unverified),
    ("security-review", &[], "Complete a security review of the pending changes on the current branch", "", Unverified),
    ("usage-credits", &[], "Configure usage credits or request them from your admin when you hit a limit", "", Unverified),
    ("extra-usage", &[], "Renamed to /usage-credits", "", Unverified),
    ("usage", &["cost", "stats"], "Show session cost, plan usage, and what's contributing to your limits", "", Dialog),
    ("insights", &[], "Generate a report analyzing your Claude Code sessions", "", Unverified),
    ("recap", &[], "Generate a one-line session recap now", "", Unverified),
    ("skill-doctor", &[], "Show which loaded skills are unused and costing context", "", Unverified),
    ("goal", &[], "Set a goal — keep working until the condition is met", "", Unverified),
    ("design-consent", &[], "Grant Claude agent access to your Design projects", "", Unverified),
    ("design-revoke", &[], "Revoke Claude agent access to your Design projects", "", Unverified),
    ("list-agents", &["peers"], "List subagents, teammates, and other Claude sessions you can message", "", Unverified),
    ("team-onboarding", &[], "Help teammates ramp on Claude Code with a guide from your usage", "", Unverified),
    ("help", &[], "Show help and the available commands", "", Dialog),
    ("status", &[], "Show the session status", "", Dialog),
    ("hooks", &[], "Manage hooks", "", Dialog),
    ("theme", &[], "Change the terminal color theme", "", Dialog),
    ("login", &[], "Sign in to your account", "", Changes),
    ("logout", &[], "Sign out of your account", "", Changes),
    ("resume", &[], "Resume an earlier conversation", "", Changes),
    ("memory", &[], "Edit memory files", "", Dialog),
    ("ide", &[], "Manage the IDE integration", "", Dialog),
    ("cd", &[], "Change the session's working directory", "", Changes),
    ("branch", &[], "Branch the conversation", "", Changes),
    ("fork", &[], "Fork the conversation", "", Changes),
    ("background", &[], "Move the session to the background", "", Changes),
    ("add-dir", &[], "Add a working directory", "", Changes),
    ("permissions", &[], "Manage tool permission rules", "", Dialog),
    ("plugin", &[], "Manage plugins", "", Dialog),
    ("exit", &[], "Exit the session", "", Changes),
    ("rewind", &[], "Rewind the conversation to an earlier point", "", Changes),
    ("restart", &[], "Restart the session", "", Changes),
    ("remote-control", &[], "Connect this session for remote control", "", Unverified),
    ("statusline", &[], "Set up the status line", "", Unverified),
];

/// The class and reason of a native row.
fn class_of(handling: Handling) -> (CommandClass, Option<&'static str>) {
    let terminal = |reason| (CommandClass::TerminalOnly, Some(reason));
    match handling {
        Handled => (CommandClass::RunWithHandling, None),
        Changes => terminal("Changes the session outside the planner"),
        Dialog => terminal("Opens a terminal dialog"),
        SavesDefault => terminal("Saves your default for new sessions"),
        Unverified => terminal("Not verified in the planner"),
    }
}

/// Whether `name` is a native command's name or alias: Claude Code runs its native command for
/// it, whatever a listed row calls itself.
pub(super) fn is_native(name: &str) -> bool {
    let named = |&(native, aliases, ..): &Native| native == name || aliases.contains(&name);
    NATIVES.iter().any(named)
}

/// The native rows, in table order.
pub(super) fn native_entries() -> Vec<CommandEntry> {
    NATIVES
        .iter()
        .map(|&(name, aliases, description, argument_hint, handling)| {
            let (class, reason) = class_of(handling);
            CommandEntry {
                name: name.to_owned(),
                aliases: aliases.iter().map(|alias| (*alias).to_owned()).collect(),
                description: description.to_owned(),
                argument_hint: argument_hint.to_owned(),
                source: CommandSource::BuiltIn,
                class,
                reason: reason.map(str::to_owned),
            }
        })
        .collect()
}
