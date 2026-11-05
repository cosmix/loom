# V2: the Hearth panel frame, head, empty state and composer, and the Chat section of DESIGN.md (R13, look)

Tier: fable (`loom-senior-software-engineer` with the model override `fable`). Wave 2, beside V1 (transcript).

Before anything else:
1. Invoke `frontend-design:frontend-design` with the Skill tool.
2. Read `../common.md`, `web/src/plan/ui/DESIGN.md`, the mockup `doc/plans/mockups/chat/alt-d-hearth.html` (the panel, head and composer of all five states, and state 1 whole) and the mockup shots the main agent passes you (`<scratch>/mockup/state-<n>-<theme>.png`), then N1's wave-1 report (head markup, `EmptyChat`, the composer views).

The mockup is the specification; its CSS is a reference. Build with the planner's tokens and the real components; no colour literal outside `tokens.css`. The user's overrides (common.md) beat the mockup: no avatar in the head and no roundel there, no ⋯ menu, no idle "Waiting for you"; the pane's one roundel lives in the transcript (V1, T1).

## Fixed targets (what the shots must show, Walnut and Ledger)

1. **Panel** (`chat.css`, `chat-surface.css`): `--card` surface with a hairline start edge. The hearth glow: one radial gradient of `--primary` at 14% (`color-mix`) fading to transparent, 520 by 260px, centred on the panel's foot 120px below its bottom edge, behind the composer, `pointer-events: none`, in every state. `chat.css` keeps no `box-shadow`, `opacity` or `--tone-` (`panel-a11y.test.tsx`), so the glow and every new rule go in `chat-surface.css`. Chat-wide tokens and the `data-hearth` hook are scoped on `.chat-panel`, `.chat-transcript` and `.chat-composer` together, because `Transcript` and `ComposerArea` also mount on `/new` and in the help scenes.
2. **Head** (`panel-head.tsx` markup from N1, styles here): padding 16px `--chat-gutter` 12px. The title 15px/600 on one line with an ellipsis; "New chat" (`h2[data-new]`) at 500 in `--muted-foreground`. Under it the sub-line at `--type-ui` in `--muted-foreground`, its state word in `--foreground`, one line with an ellipsis. At the end: New chat as a 32px round quiet icon button, then Close in the sheet. Nothing else: no state glyph beside the title, no run line, no time.
3. **Empty state** (`empty-chat.tsx`, state 1): bottom-aligned above the composer, left-aligned, gap 12px. The title in `--font-reading` 26px/1.2, 500, letter-spacing -0.015em, `max-width: 14ch`; the line under it in `--muted-foreground`, 38ch. The context prompts (`context-prompts.tsx`, its `ul aria-label="Suggested prompts"` and button names unchanged; frozen `context.contracts`) draw as pills under it: 30px high, 12px inline padding, full radius, 1px `--border`, 13px/500 `--foreground`; hover fills `--muted` and lifts 1px. On a screen with no prompts the pills are absent and nothing else moves.
4. **Composer** (`composer.tsx`, `composer-concluded.tsx`, `chat-composer.css`, `stop-control.tsx`, `attach-document.tsx`): the field is `.chat-composer-box`: `border: 1px solid var(--border)` (frozen "the composer has a visible border": keep that exact declaration form and never a 0 or transparent border), radius 22px, `--background` fill, padding 10px 8px 8px 16px, a grid of the text and the tools. The textarea stays a direct child of the box, 15px/1.5, min-height 44px, no border or outline of its own. Focus within the box: the border turns `--ring` with a 2px `--ring` outline at 2px offset. The attached chips and the context chip keep their row anatomy (`chat-chips.css`) above the text, inside the box. The tools sit inside the box at its end, centred on one line: the paperclip as a 32px round quiet button, Stop (while a turn runs) as a 32px round secondary button with a filled rounded square glyph, Send as a 32px round primary button with lucide `ArrowUp`. Their accessible names stay "Attach a file or document" (`attach-document.tsx`, rendered by `new/document-starter.tsx` `DocumentStarter` with `className="chat-attach"`), "Stop" and "Send", by sr-only text inside each button. Their container keeps `.chat-composer-actions` (frozen polish contract "Stop sits in the composer's action row"; help pins `.chat-composer-actions .chat-attach`, `.chat-composer-send button`, `.chat-composer-actions .chat-composer-stop button`). A blocked Send's reason and the composer notes go to a foot line under the box at `--type-fine` in `--muted-foreground`, which otherwise reads "Enter to send" (idle) or "Your message waits for the turn to finish" (working or a decision pending). While a file is dragged over, the box shows "Drop to attach" with no layout shift (as now).
5. **Ending and ended composer** (states 4 and 5): `.chat-composer-ending` and `.chat-composer-ended` are one line, italic `--font-reading` 15px/1.4 in `--muted-foreground`; ended puts "Start a new chat" after it as a 32px primary pill (`--primary` fill, `--primary-foreground`, 600) over the glow.
6. **Buttons**: add `shape?: "pill" | "round"` to `DisabledReason` (`web/src/plan/ui/disabled-reason.tsx`), drawn as `data-shape` on the `.plan-button`, with the rules in a new `web/src/plan/ui/hearth.css` (full radius; `round` is square at the control height with no inline padding). Shapes are used only in the chat and on `/new` (DESIGN.md, Chat). `hearth.css` also holds the suggestion pill (`.hearth-pill`) so `/new` can reuse it; the context prompts use it.
7. **Motion**: the empty state and the composer enter with V1's rise (8px, 460ms `cubic-bezier(0.16, 1, 0.3, 1)`); pills and round buttons scale to 0.98 on press; all off under `prefers-reduced-motion`.
8. **390px**: the sheet holds the same head, field and foot; nothing scrolls sideways.

## DESIGN.md

Add a `## Chat` section after Status. It states, as rules with their reason:
- the user's messages are bubbles on the right (`--muted`), the assistant's replies run on the left in the reading face, and no avatar is drawn;
- pill and round buttons (`DisabledReason shape`) exist only in the chat and on `/new`; the chat may show more than one primary (Send, the permission card's Allow, Start a new chat), one per surface;
- the hearth glow at the panel's foot is the one glow in the planner;
- the working card's shimmer and the wrap-up card's drawn checks;
- **one roundel per pane**: the roundel is an activity signal, never an avatar; at most one is drawn in the chat pane, on the turn that is working now, the starting card or the wrap-up card while the closing write runs, and none when nothing works;
- motion is off under `prefers-reduced-motion`.
Update the Type paragraph's chat sentence (head title and sub-line, the empty-state title at 26px in the reading face, the left-behind title at 20px) and delete the exceptions that describe the chat menu (`.chat-menu-item`, `.chat-menu-pop`). planner-visual owns Colour and Status: do not edit them.

## Help scene

`web/src/plan/help/chat-scene.tsx`: keep every pin target matching exactly one element (`chat-scene.test.tsx`); move a selector with its element and keep its note true. If the conversation scene's notes name a removed element, retarget them.

## Files

Your row in the stage's worker table. Never edit V1's files (transcript, items, tool rows, permission card, wrap-up cards, turn footer, `chat-transcript.css`, `chat-decision.css`, `chat-notices.css`). Frozen `polish.contracts.test.tsx` mocks `./start-chat`, `./end-chat`, `./run-header`, `./transcript`, `./attention` and `@/plan/review/changed-since` with fixed export names: anything the panel, head or composer imports from those modules must be one of `NewChatButton`, `StartingBrief`, `EndChatMenuItem`, `useCanEndChat`, `RunHeader`, `Transcript`, `useAttention`, `ChangedSinceControl`.

## Run once at the end

`node_modules/.bin/vitest run src/plan/chat src/plan/ui src/plan/help src/plan/new`, from `web/`.

## Report

Files changed; the test result; every test whose assertion you changed and why; each fixed target you could not meet and why; the states and widths the main agent must shoot to judge each target; the `hearth.css` classes and the `shape` prop, for the later `/new` stage.
