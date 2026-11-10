# W5: guidance and spec text (step B6)

Read `common.md` in this directory first. You are a `loom-software-developer`. You run in wave 1 beside W1. You edit Markdown only and run no build or test.

## Files owned

`loom/src/planning/guidance/text/common_brief.md`, `doc/product/feat-planner/section_07.md`, `section_08.md`, `section_09.md`, `section_12.md`.

## The rule the text now states

Slash commands typed in the planner chat run as they do in the terminal, except those the planner marks terminal only. The chat's composer lists the session's skills and commands when the message starts with `/`. Terminal-only commands are listed but disabled, each with its reason: "Changes the session outside the planner" (`/clear`, `/resume`, `/exit` and the like), "Opens a terminal dialog" (`/usage`, `/model`, `/help`, `/status` and the like), "Saves your default for new sessions" (`/effort`), "Not verified in the planner" (the other native commands). A name not in the session's list is refused with "Not in this session's command list". `/compact` and `/context` run, and their output shows in the chat. One session keeps one model: `/model` and `/effort` are terminal only because they open a dialog or save the user's default.

## Steps

Find each passage by its quoted text (line numbers drift), rewrite it to the rule above in the file's own style, and change nothing else:

1. `common_brief.md`: the sentence "Never suggest a typed slash command: the app refuses them." becomes "Slash commands run as in the terminal, except those the planner marks terminal only." Keep the rest of its paragraph as it is.
2. `section_07.md`: "Chat messages that start with `/` are refused with a note, except `/compact`." and the "(My call: `/clear` …)" note after it: state the rule and why terminal-only commands stay off (they change the session outside the bridge or open a dialog the browser cannot see, and a dialog left open captures the next pasted Enter).
3. `section_08.md`: the delivery-state table row "until `UserPromptSubmit`" (a local command such as `/context` or `/compact` is delivered when its own record or `PreCompact` arrives, and holds the queue until it settles), and "`/` other than `/compact` disables Send with the note …" (Send is disabled with the command's reason for a terminal-only or unlisted command; a slash message carries no attachments and one line).
4. `section_09.md`: "slash commands typed by the user are refused by the app, so never suggest one." (the sentence wraps across two lines) becomes the B-D6 rule; "One session keeps one model: the bridge refuses `/model` with every other slash command" keeps its first clause and names the new reason (terminal only: a dialog, or the user's saved default).
5. `section_12.md`: "refuses slash commands other than `/compact` (section 07)." names the class gate instead.
6. Check your work: from the repository root, `rg -nF` each quoted old fragment over its file finds nothing ("Never suggest a typed slash command", "are refused with a note, except", "other than /compact disables Send" with backticks around /compact, "slash commands typed by the user are refused", "with every other slash command", "refuses slash commands other than"), and every heading, table and list in the files still parses as before.

## Done when

The six old fragments are gone, each rewritten passage states the rule above, and the main agent's markdownlint run over the five files passes.

## Traps

- `common_brief.md` is guidance compiled into the binary; another sentence of it may have changed in the operator's uncommitted work. Change only the one sentence.
- No AI attribution, no new headings, no dates.
