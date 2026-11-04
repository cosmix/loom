# V2 — rethink how the chat panel looks and sits on the page (issues 7, 9 type, 8 look)

Tier: opus, effort medium (`loom-senior-software-engineer`). First invoke `frontend-design:frontend-design` with the Skill tool. Then read `common.md` (Design rules, Visual work), `web/src/plan/ui/DESIGN.md`, `doc/loom/knowledge/architecture/planner-web-shell.md` ("Chat home and the chat panel"), and this brief.

## The operator's complaint

The chat feels bolted on:

- the collapsed rail's icon is not inviting;
- "About section 4" sits at the top of the panel;
- the transcript type is small;
- tool activity shouts;
- there is no inviting place to drop a file.

C1 changes behaviour in parallel (fold line, Skill drop, chip labels, uploads, Add to chat opening the panel). You own the presentation.

## Fixed targets

- **Collapsed rail** (`chat/panel.tsx` `CollapsedRail`, `chat-state-glyph.tsx`): a labelled affordance, not a bare glyph. The word "Chat" is set vertically, or as a compact labelled tab, with `ChatGlyph`, and the state is a word ("Waiting for you", "Working") when one applies. It is one click to open and keyboard reachable. Keep its accessible name.
- **Context** (`chat/context-line.tsx`): no line at the top of the panel. The context becomes a quiet chip inside the composer (`composer.tsx` renders `ContextLine`), above or beside the textarea: "About section_05" with a dismiss control. Keep `role="group"` and `aria-label="Viewing <name>"`, the dismiss button's name "Dismiss context" (journey J4), and the composer placeholder that contains the name (frozen `chat/context.contracts.test.tsx`).
- **Type** (`chat/chat-transcript.css`, `chat-surface.css`; C1 removes the `text-[13px]` class from `transcript.tsx`, which is C1's file, so set every size in CSS):
  - assistant prose stays at the reading size in `--font-reading`;
  - user turns at least 14px;
  - the fold line and tool rows at least 13px (type floor);
  - the kickoff transcript inherits these. It must not shrink below the panel.
- **Fold line** (C1's markup): one quiet line in `--muted-foreground`, glyph then words, one height. It expands in place to the rows, which stay in the `<details>` DOM while folded (frozen "chat expanded tool row").
- **Chips** (`chat/chips.tsx` markup is C1's label, the CSS is yours):
  - a selection chip shows the quote on one line, with the source as `--type-fine` secondary text;
  - a file chip shows its name and size; an image chip shows a 32px thumbnail.
  - Chips wrap. They never scroll sideways at 390px.
- **Drop target**: spread C1's `useFileIntake()` (from `composer-hooks.ts`) on the composer box in `composer.tsx`. While `dragging` is true the composer shows "Drop to attach" inside its bordered box, with no layout shift. The frozen contract "the composer has a visible border" holds.
- **Head** (`panel-head.tsx`): title, state word, menu and collapse on one centre line.

## Exceptions

The kickoff stage's main agent adds your deliberate DESIGN.md exceptions (you do not own DESIGN.md in this stage). List each one in your report as `exception: <surface>: <rule>: <why>`.

## Files

Yours are listed in the plan YAML worker table. You never edit C1's files.

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/help/chat-scene.test.tsx` from `web/`.
