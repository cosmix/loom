# V3: the command menu, the commands atom, the chip and the web gate (steps B3 web, B5; Decisions B-D2 "Web gate", B-D5)

Read `common.md` in this directory first. You are a `loom-senior-software-engineer` running on the fable model: the menu is a new visual and keyboard surface. Load the `frontend-design:frontend-design` skill before your first edit. You run in wave 1 beside V1 (the chat head: disjoint files).

## Files owned

`src/plan/chat/command-menu.tsx`, `command-menu.css`, `command-menu.test.tsx`, `command-chip.tsx` (new), `src/plan/state/commands-atom.ts`, `src/plan/state/commands-atom.test.ts` (new), `src/plan/api/socket.ts` (the `commands` case), `src/plan/api/socket-commands.test.ts` (new), `src/plan/chat/composer.tsx`, `src/plan/chat/composer-hooks.ts`, `src/plan/chat/message-problem.ts`, `src/plan/chat/message-problem-commands.test.ts` (new), `src/plan/chat/items.tsx`, `src/plan/chat/chat.contracts.test.tsx`, `src/plan/chat/composer-delivery.test.tsx`.

Frozen, read only: `src/plan/chat/commands.contracts.test.tsx` (contracts `slash vectors get the server answers in the composer gate` and `typing a slash opens the command list and keeps focus`). `chat.contracts.test.tsx` is an earlier plan's frozen contract file: you edit only its slash test, as the plan's "Planned edits" table says.

## What exists (verified 2026-11-10; anchor by text)

- `chat/message-problem.ts:8-21` `messageProblem(text, attachments)`: strips `CONTROLS` and leading white space and U+FEFF (`:13`), refuses any slash message but `/compact` with "Slash commands are not available here; `/compact` is." (`:14-16`), then the 100,000-byte cap and the empty check. Callers: `composer.tsx` (`:80`, the send reason) and `composer-hooks.ts:131` (`problemWith`).
- `composer.tsx`: the textarea's `onKeyDown` (`:117-122`) sends on Enter without Shift or composition; `ComposerActions` (`:157-`) holds the send row; `ComposerArea` is exported at `:287`. `composer.tsx` (331 lines) and `composer-hooks.ts` (317) gain only wiring.
- `attachments/floating-position.ts:44-48` `useFloatingPlacement(rect) -> {ref, placement}` measures on render only and flips above near the bottom; `attachments/add-to-chat.tsx:94` keeps the selection with `onMouseDown={(event) => event.preventDefault()}`. Below 1280px the chat is a Radix Dialog sheet (`chat/panel.tsx` `ChatSheet`), so the menu renders inline in the composer subtree with `position: fixed`, never in a portal.
- `chat/items.tsx:176-` `UserItem` renders the user's text through `Untrusted`; the user item has `command: {name, args} | null` (merged).
- `chat/chat.contracts.test.tsx:262-270` pins the note "Slash commands are not available here"; `composer-delivery.test.tsx:212-222` refuses `/compactly` and disguised `/clear` with the old note.
- `connectionEpochAtom` (`state/atoms.ts`) bumps on every resync; `sessionAtom` (`chat/model.ts:25`).

## Steps

1. **`commands-atom.ts`**: `commandsAtom`, the commands view of the chat session: `{session, state: "loading" | "ready" | "unavailable", generation, reason, entries}`. It fetches `GET /api/session/commands?session=<id>` (`apiGet`, `api/client.ts`) on a session change and on every `connectionEpochAtom` bump, and takes a `commands` frame for the same session (the socket stores it; a frame for another session is ignored; a lower generation never replaces a higher one). With no session it holds the `loading` view the route gives for no session. Parse every answer with `commandsViewSchema`.
2. **`socket.ts`**: a `commands` case in `apply` that hands the frame to the atom's setter.
3. **Gate.** `messageProblem(text, attachments, commands)`: after the existing stripping, a slash message is refused with the server's reasons (`slash-server`'s `check_send`; `common.md` of `slash-server` lists the texts): Terminal only by its row's reason; a non-native name by `Not in this session's command list` when ready, `Commands are still loading` when loading, `This session's command list is unavailable` when unavailable; more than one line or attachments by `Send a command alone, on one line`; over 300 characters by `Too long for a command`; names matched through aliases. Plain text is never affected by the commands state. Both callers pass the atom's value. The server's check stays authoritative.
4. **`CommandMenu`** (`command-menu.tsx`, `command-menu.css`): a `/` typed as the first character (after the stripping) opens a `role="listbox"` of every entry, filtered by the typed name; the textarea becomes `role="combobox"` with `aria-expanded`, `aria-controls` and `aria-activedescendant`; focus stays in the textarea. Up and Down move the highlight; while the list is open, Enter and Tab insert `/<name>` and a space for the highlighted entry (the textarea's Enter sends only while it is closed); Escape closes it. Each option shows the name, the source word (built-in, bundled, plugin, personal, account or skill), the description and the argument hint, all through `Untrusted`; a disabled (Terminal only) entry shows its reason and cannot be inserted. The list is anchored with `useFloatingPlacement`, sets its own max-height and overflow, recomputes on scroll and resize (it measures on render only), flips above at the panel foot, and its options call `preventDefault` on mouse-down. A "Commands" button in `ComposerActions` opens the same list; picking an entry inserts `/name` and a space, with the argument hint as the textarea's placeholder.
5. **The chip.** `command-chip.tsx`: a user item with `command` set renders the command as a chip (name and args, through `Untrusted`) in `UserItem`, in the chat's chip style (`chat-chips.css`).
6. **Planned edits**: `chat.contracts.test.tsx`'s slash test expects the class reason note for `/clear` ("Changes the session outside the planner"); `composer-delivery.test.tsx`: `/compactly` refused as `Not in this session's command list`, the disguise cases kept with the new wording. Keep every other assertion; the stage's INTEGRITY line names these two events.
7. **Tests** (new files): `command-menu.test.tsx` (filtering; keyboard selection with Enter, Tab and Escape; disabled entries show their reason; each source word shows; the button opens the list; Send emits the literal `/name args`); `commands-atom.test.ts` (a `commands` frame replaces the list, a stale generation does not; a session change and an epoch bump refetch); `socket-commands.test.ts` (a frame through the socket handler reaches the atom); `message-problem-commands.test.ts` (`unavailable` keeps plain-text Send enabled; attachments on a slash message are refused). Run `node_modules/.bin/vitest run src/plan/chat src/plan/state src/plan/api`.

## Done when

The two frozen contracts and your tests pass, `rg -nF 'Slash commands are not available here' src/plan` finds nothing, and `composer.tsx` and `composer-hooks.ts` stay under 400 lines.

## Traps

- Radix Popover takes focus; the menu must not.
- `getByRole` takes a name string and no `exact`; jest-dom is not installed.
- A menu option is a `role="option"` with `aria-disabled` for a disabled entry, never the `disabled` attribute (it would drop out of the accessibility tree).
