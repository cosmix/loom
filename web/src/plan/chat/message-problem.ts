import type { Attachment } from "@/plan/api/schema-chat";
import type { CommandEntry, CommandsView } from "@/plan/api/schema-session-commands";
import { formatMessage } from "./format-attachments";
import { CONTROLS } from "./model-identity";

const CAP = 100_000;
/** The longest slash message the chat sends, in characters (the server's `MAX_SLASH_LEN`). */
const MAX_SLASH_LEN = 300;
const ALONE = "Send a command alone, on one line";
/** The refusal of a name the list has no row for, by the state of the list. */
const UNLISTED: Record<CommandsView["state"], string> = {
  ready: "Not in this session's command list",
  loading: "Commands are still loading",
  unavailable: "This session's command list is unavailable",
};
/** A terminal-only row without a reason of its own. */
const UNVERIFIED = "Not verified in the planner";

// U+FEFF is neither a control character nor white space, and `String.prototype.trim` strips it.
const LEAD = new RegExp(`^[\\p{White_Space}${String.fromCodePoint(0xfeff)}]+`, "u");
const TRAIL = /\p{White_Space}+$/u;
const SPACE = /\p{White_Space}/u;
/** The first word of a slash message: its `/` and name. */
const TYPED = /^[^\p{White_Space}]*/u;

/**
 * `text` from its `/`, as the pane receives it: the control characters the pane strips are gone
 * and leading white space and U+FEFF are skipped, so neither can hide a command. Null for a
 * message that does not start with `/`.
 */
function slashText(text: string): string | null {
  const lead = text.replace(CONTROLS, "").replace(LEAD, "");
  return lead.startsWith("/") ? lead : null;
}

/** The row `name` (no slash) runs: matched by name, then by alias, as the server's table does. */
function findCommand(commands: CommandsView, name: string): CommandEntry | undefined {
  return (
    commands.entries.find((entry) => entry.name === name) ??
    commands.entries.find((entry) => entry.aliases.includes(name))
  );
}

/** Why a row cannot be sent from the planner; null for a row that can. */
export function terminalOnlyReason(entry: CommandEntry): string | null {
  return entry.class === "terminal-only" ? (entry.reason ?? UNVERIFIED) : null;
}

/**
 * The name being typed: what follows the `/` while the message is a slash and its name. Null
 * for any other message, and once white space follows the name (the arguments began).
 */
export function typedCommandName(text: string): string | null {
  const slash = slashText(text);
  return slash === null || SPACE.test(slash) ? null : slash.slice(1);
}

/** The rows a typed name keeps, in the list's order: names and aliases that start with it, then the rest. */
export function matchingCommands(commands: CommandsView, typed: string): CommandEntry[] {
  const wanted = typed.toLowerCase();
  if (wanted === "") return commands.entries;
  const names = (entry: CommandEntry) =>
    [entry.name, ...entry.aliases].map((name) => name.toLowerCase());
  const has = (entry: CommandEntry, test: (name: string) => boolean) => names(entry).some(test);
  const leading = commands.entries.filter((entry) => has(entry, (name) => name.startsWith(wanted)));
  const inside = commands.entries.filter(
    (entry) => !leading.includes(entry) && has(entry, (name) => name.includes(wanted)),
  );
  return [...leading, ...inside];
}

/**
 * The draft once the command `name` is picked: the command and a space, then what the draft
 * already said as its arguments (the words after a typed name, or the plain text), so a pick
 * loses nothing.
 */
export function withCommand(text: string, name: string): string {
  const slash = slashText(text);
  const rest = slash === null ? text : slash.replace(TYPED, "");
  return `/${name} ${rest.trim()}`;
}

/** The argument hint of the command the draft names, while its arguments are due and empty. */
export function pendingArgumentHint(text: string, commands: CommandsView): string | null {
  const name = /^\/(\S+) $/u.exec(text)?.[1];
  const hint = name === undefined ? "" : (findCommand(commands, name)?.argument_hint ?? "");
  return hint === "" ? null : hint;
}

/**
 * Why the slash message `line` cannot be sent, in the server's words and order (`check_alone`,
 * then `check_send`, `plan_session/input.rs`); null when it can. The server's check decides.
 */
function slashProblem(line: string, attachments: number, commands: CommandsView): string | null {
  if (attachments > 0 || line.includes("\n")) return ALONE;
  if ([...line].length > MAX_SLASH_LEN) return "Too long for a command";
  const name = line.slice(1).split(SPACE, 1)[0] ?? "";
  const entry = findCommand(commands, name);
  return entry ? terminalOnlyReason(entry) : UNLISTED[commands.state];
}

/**
 * Why a message cannot be sent as written, or null when it can. The composer and the send gate
 * share it. `commands` decides a slash message only: plain text never depends on its state.
 */
export function messageProblem(
  text: string,
  attachments: Attachment[],
  commands: CommandsView,
): string | null {
  const slash = slashText(text)?.replace(TRAIL, "");
  if (slash !== undefined) {
    const problem = slashProblem(slash, attachments.length, commands);
    if (problem !== null) return problem;
  }
  const formatted = formatMessage(text, attachments);
  // The pane strips C0/C1 controls after formatting, preserving newline and tab.
  const delivered = formatted.replace(CONTROLS, "");
  const bytes = new TextEncoder().encode(delivered).byteLength;
  if (bytes > CAP)
    return `Message with attachments is ${bytes - CAP} byte${bytes - CAP === 1 ? "" : "s"} over the 100,000-byte limit.`;
  if (!delivered.trim()) return "Write a message or attach a passage";
  return null;
}
