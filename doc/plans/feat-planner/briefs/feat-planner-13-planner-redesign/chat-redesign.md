# Stage chat-redesign

Read `common.md`, then `web/src/plan/ui/DESIGN.md`. Paths are under `web/src/plan/`. Every worker invokes `frontend-design:frontend-design` before writing code.

The operator: "I particularly don't like how chat looks and feels. The styling is horrible. Being docked on the right doesn't bother me, but it needs to be contextual, i.e. understand what the user is seeing and adapting to it. Also, the enclosed parts within the chat look jarring. We need to beautify it."

## C1: the panel's look (fable, explicit model override at spawn)

A designer's pass over the whole panel, on the panel's real structure.

**Component tree:** `dock.tsx` to `chat/panel.tsx`:

- `SessionBody` holds `panel-head.tsx`, `state-line.tsx`, `run-header.tsx`, `connection-banner.tsx`
- `transcript.tsx` holds `items.tsx`, `tool-row.tsx`, `patch-view.tsx`, `turn-footer.tsx`, `permission-card.tsx`
- `composer.tsx` holds `chips.tsx`, `queue-strip`, `suggestion-cards.tsx`, `new-chat-offers.tsx`, `stop-control`

**The enclosed parts** to dissolve:

- permission card (`permission-card.tsx:343`)
- pending chips (`chips.tsx:212`)
- rebuild chip (`chat-surface.css:237`)
- `HazardPanel` (`state-line.tsx:114,135`)
- the composer box (`chat-surface.css:269`)
- the user bubble (`chat-surface.css:203-211`)

Turn the panel into one continuous surface where hierarchy comes from type, space and a single hairline. A user turn is distinct without being a bubble in a box. Tool activity is a quiet run of lines that folds. A permission decision is the one thing allowed real emphasis, and even that does not nest boxes. The composer is part of the panel with no card floating in it. Assistant prose uses the reading face, as `doc/product/feat-planner/section_08.md` states.

**Constraints:**

- New CSS goes in `chat/chat-surface.css`; `chat.css` may not gain `box-shadow`, `opacity` or `--tone-` (`panel-a11y.test.tsx:179-183`).
- `panel.test.tsx:171,177` pins the grid rows.
- Live regions stay out of `aria-hidden` and keep `role="status"` (`conventions/plan-web.md`).
- Keep the `y`/`n` permission keys and the `DenyNoteForm`.
- Check against `chat/transcript.test.tsx` and `items.test.tsx`; a test that pins a removed wrapper changes, and you list the lost guard.
- The auto-mode notice ("Auto mode blocked Bash: …") renders through `NoticeItem` (`items.tsx`); give it a tone that reads as "the session was stopped here", not as an error card.
- Leave a slot in `panel-head.tsx` for C2's context line and name it in your report.

C1 finishes before C2.

One check: `node_modules/.bin/vitest run src/plan/chat/panel src/plan/chat/items`.

## C2: context (sonnet)

Nothing tells the chat what the user is viewing today. `screenAtom` (`chat/session-home.ts:17`) holds only `{feature, plan}`, filled by `ScreenSync` (`chat/screen-sync.tsx`), and `discussDoc` (`files/doc-heading-view.tsx:180`) attaches a document only on a click.

**Viewed document.** In `chat/viewed-document.ts`:

```ts
export type ViewedKind =
  | "vision" | "product-overview" | "section" | "metaplan"
  | "engineering-overview" | "plan" | "pressure" | "run" | "file" | null;

export type ViewedDocument = {
  kind: ViewedKind;
  /** Repo-relative source path to attach, or null when the page has none (pressure, run). */
  path: string | null;
  /** Short name for the chip: "section_05", "Product overview", "metaplan". */
  name: string;
};

export function viewedOfPath(pathname: string, checklist: ChecklistView | null): ViewedDocument;
export const viewedDocumentAtom: Atom<ViewedDocument>;
```

`ScreenSync` keeps it current. The source paths:

| Kind | Path |
| --- | --- |
| `product-overview` | the vision |
| `engineering-overview` | the first metaplan |
| `section` | the section file |
| `plan` | `plan_rows[].path` |
| `file` | the file address |

**Panel behaviour:**

1. A quiet context line in the panel head, `chat/context-line.tsx`, has the accessible name `Viewing <name>` and reads `About section_05`, with a button named `Dismiss context`. It follows navigation immediately, whether or not a chat is running. Dismissing it clears the context for that document until the user navigates to another one. Prompts render in `chat/context-prompts.tsx`, in a list named `Suggested prompts`.
2. The composer placeholder names it: `Ask about section_05, or say what to change`. With no context: `Ask about this feature`.
3. With no session and an empty draft, the panel shows two or three prompts for the kind, from this table. A click fills the composer, focuses it and sends nothing, so no click spends tokens:

| Kind | Prompts |
| --- | --- |
| vision | Which sections does this imply? · What is still undecided? |
| product-overview | What are the main decisions here? · What is still undecided? |
| section | What does this section decide? · What does this conflict with? · Change this section so that |
| metaplan | Is the plan order right? · Which plan carries the most risk? |
| engineering-overview | What are the main engineering decisions? · What could go wrong? |
| plan | What could go wrong with this plan? · Does this cover what its sections ask for? · Change this plan so that |
| pressure | Which plan should be tested first? |
| run | What did the build change compared to the spec? |
| file, null | none |

4. **Sending.** When the viewed document has a path and it differs from the last one attached in this session, `useSendDraft` (`chat/composer-hooks.ts:~112`) prepends a source attachment for it, through the same `AttachmentDraft` that `discussDoc` builds. Extract that into `attachments/doc-attachment.ts` (`docAttachment(doc)`), fetch the document through `files/api.ts`, and let `discussDoc` use it too. There are no wire changes: the attachment already carries the path (`format-attachments.ts`), and the server's `Message{paths}` already picks Plan or Spec guidance from the first attachment (`planning/guidance/start.rs:181`). A failed fetch sends the message without the attachment and shows nothing alarming. A passage the user selected and added stays first.
5. **Session home stays.** `sessionHereAtom` and the "elsewhere" notice are unchanged. Context changes within the session's own screen. When a chat is running, the line reads `Now viewing section_05`, and the next message carries it per point 4.

**Offers leave the panel:** remove `PrimaryOffer` from `state-line.tsx:181` and `new-chat-offers.tsx`; the file keeps `NewChatButton`. Update `panel-offers.test.tsx`, `session-home.test.tsx`, `chat.contracts.test.tsx`, listing every lost guard. After a concluded chat, the panel offers New chat and the prompts.

**Tests:** `chat/viewed-document.test.ts`, `chat/context-line.test.tsx`, `chat/context-prompts.test.tsx`, and `composer-hooks` coverage of point 4.

One check: `node_modules/.bin/vitest run src/plan/chat`.

## Main agent

After the workers:

1. Run `node_modules/.bin/oxfmt`.
2. Build, then run the journey script and a panel screenshot pass. Session views need socket frames patched in the browser, because a stage has no Claude login (`conventions/plan-web.md`), or the fake claude in `journeys.md`; say in the review which views were synthetic.
3. Run one `loom check chat-redesign` beside the final `loom-code-reviewer` round, whose brief carries `DESIGN.md`, this brief and the screenshot paths.
