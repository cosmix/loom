# chat-web W3: transcript items and tool rows

Dispatch: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 600000 ms. The orchestrator partitions this territory into ordered file-sized forwards before dispatch to fit the 540 s deadline; workers never delegate. All test/proof commands below are for the orchestrator; workers run compile-only checks, no git, and never write under `.loom/`.

Lane note: `tool-group.ts` and its test are codex unit C6; the section below on it is reference.

Working dir `web/`. You own `src/plan/chat/{transcript.tsx, transcript.test.tsx, items.tsx,
tool-row.tsx, tool-row.test.tsx, patch-view.tsx}`; codex unit C6 wrote `tool-group.ts` (`groupToolRows`), which you import. W1's
atoms exist. Frozen contracts: `src/plan/chat/chat.contracts.test.tsx`.

## transcript.tsx

`<Transcript />`: `role="log"`, `aria-live="polite"`, each new assistant text block announced
once (tool rows are not). Turns oldest first; the 20 most recent in full, older ones one line
each ("Turn 3 · 2 files changed · 4.1k tokens") expanding in place. Each turn: its items, then
W5's `<TurnFooter turn />`. Follows new items while scrolled to the bottom; when the reader
scrolled up, new items do not move it and a "New activity" pill at the bottom edge scrolls down.
No placeholder bubble while working: the state line says so. An ended conversation stays above
the new session's under a divider with its end time.

## items.tsx

Per section 08 "Message rendering":

- `user`: right-aligned on `--muted`, rounded, max 85% width, text as plain text with line
  breaks preserved, then W4's `<SentChips item turn />`. A user item with `origin` `offer` or `start`
  (the session's opening message; label from plan 8's `SessionBriefView.label`, e.g. "Write
  section 08 — Chat interface" or "New chat") or `closing` (label "Closing message from loom
  plan") renders collapsed to its first line with the label and a Show control; `origin: null`
  (the user's own first message) renders as an ordinary user message.
- `text`: full-width through plan 3's renderer (raw HTML never rendered, comments dropped, links
  per section 06: `doc/` paths open file mode in the reading column, `http(s)` in a new tab with
  `rel="noopener noreferrer"`, anything else plain text; a chat reply never navigates away).
- `thinking`: one muted line "Thought for <n>s"; consecutive markers in one message merge.
- `tool`: `<ToolRow />`; `permission`: W4's `<PermissionCard item inline />`; `notice`: centred
  muted line with a glyph, API errors in `--tone-blocked`.
- Hover controls on user and text items: copy (the Markdown source) and the time.

## tool-row.tsx, patch-view.tsx

One row: glyph, tool name, `summary` (the bridge's, as is), status: pending `BusyRoundel`, done
muted check, error `--tone-blocked` cross with the error's first line under the collapsed row,
denied "Denied" with the note, not finished muted "Not finished". Clicking expands to input and
result as monospace text (`white-space: pre-wrap`, rendered as text), the result capped at 40
lines with "Show all". `Edit`/`Write` expand to a unified diff from the result's
`structuredPatch` in `patch-view.tsx` (added lines `--tone-completed`, removed `--tone-blocked`,
plain text, never CodeMirror). `Agent` rows show the subagent's tool-call count and a
`BusyRoundel` while running; nested rows (`subagent` set to its id) render under it indented one
step, collapsed by default; expanding also shows the subagent's final result.

## tool-group.ts

`groupToolRows(items, turnEnded): Array<Item | ToolGroup>`: only after the turn ends, three or
more consecutive tool items with no text item between fold into `{ kind: "group", items,
label }` with label "Used <n> tools · edited <m> files" (m counts distinct Edit/Write paths);
rows in error or denied stay outside the fold and break it. Tests: two rows (no fold), running
turn (no fold), error in the middle.

## Required test cases (coverage audit; named tests)

`transcript.test.tsx`: "user item layout and chips"; "thinking markers merge"; "api_error notice
toned blocked"; "stopped notice"; "html comment dropped"; "code block shows language label and
copy control" (plan 3's renderer; if it has no copy control, wrap code blocks in a small
component here that adds one); "doc link opens file mode"; "outside-doc path is text"; "http link
opens new tab with noopener"; "no placeholder while working"; "copy control copies markdown
source"; "offer origin collapsed with brief label"; "closing origin labelled Closing message from
loom plan"; "null origin renders plainly"; "older turns collapse to one line and expand"; "pill
when scrolled up, follow at bottom" (stub `scrollTop`/`scrollHeight`); "log region announces text
blocks only". `tool-row.test.tsx`: "renders summary as given"; "pending shows roundel"; "done
shows check"; "denied shows note"; "not_finished label"; "result capped at 40 lines with Show
all"; "edit expands to coloured patch"; "agent row nests subagent items collapsed and shows
result". `tool-group.test.ts`: "two rows no fold"; "running turn no fold"; "error in the middle".

## Fix-round additions (pressure test px-browser-15, -27, px-security-09)

- Assistant text renders `<Markdown source path="doc/README.md" sections={[]} headingIds={false}
  ... />`: relative links resolve against `doc/` whatever the reader has open, and no heading id
  from a reply enters the DOM.
- Tool row input, result, summary and every path render through W1's `<Untrusted>`.
- Tests: "relative and doc-prefixed links in a reply resolve against doc/" (a reply linking
  `product/section_02.md` and one linking `doc/product/section_02.md` both open
  `/files/product/section_02.md`); "reply headings carry
  no id"; "tool input with U+202E shows the marker".
