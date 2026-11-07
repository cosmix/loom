import { useAtomValue, useStore } from "jotai";
import type { JSX } from "react";

import { composeAfterAtom, focusComposerAtom } from "./composer-hooks";
import { draftAtom } from "./drafts";
import { sessionAtom } from "./model";
import { isActiveSession } from "./session-home";
import { shownSessionAtom } from "./shown-session";
import { contextDocumentAtom, type ViewedKind } from "./viewed-document";

const PROMPTS: Record<NonNullable<ViewedKind>, readonly string[]> = {
  vision: ["Which sections does this imply?", "What is still undecided?"],
  "product-overview": ["What are the main decisions here?", "What is still undecided?"],
  section: [
    "What does this section decide?",
    "What does this conflict with?",
    "Change this section so that",
  ],
  metaplan: ["Is the plan order right?", "Which plan carries the most risk?"],
  "engineering-overview": ["What are the main engineering decisions?", "What could go wrong?"],
  plan: [
    "What could go wrong with this plan?",
    "Does this cover what its sections ask for?",
    "Change this plan so that",
  ],
  file: [],
};

/** The prompts for a kind of page; none for a file or for no page. */
export function promptsOf(kind: ViewedKind): readonly string[] {
  return kind === null ? [] : PROMPTS[kind];
}

/**
 * Starting points for the page being read, shown while no chat runs and the draft is empty, as a
 * row of suggestion pills (`.hearth-pill`, ui/hearth.css) above the composer box. A click puts
 * the words in the composer and focuses it; nothing is sent, so a click costs nothing. On a
 * concluded chat it also brings the composer in place of New chat.
 */
export function ContextPrompts(): JSX.Element | null {
  const store = useStore();
  const { kind } = useAtomValue(contextDocumentAtom);
  const draft = useAtomValue(draftAtom);
  const running = isActiveSession(useAtomValue(sessionAtom));
  const prompts = promptsOf(kind);
  const empty = draft.text.trim() === "" && draft.attachments.length === 0;
  if (running || !empty || prompts.length === 0) return null;
  const fill = (prompt: string) => {
    // On a concluded chat the click chooses to write after it; see composeAfterAtom.
    const shown = store.get(shownSessionAtom);
    if (shown?.state === "ended" || shown?.state === "failed") {
      store.set(composeAfterAtom, shown.id);
    }
    store.set(draftAtom, (old) => ({ ...old, text: prompt }));
    store.set(focusComposerAtom, true);
  };
  return (
    <ul aria-label="Suggested prompts" className="chat-context-prompts">
      {prompts.map((prompt) => (
        <li key={prompt}>
          <button type="button" className="hearth-pill" onClick={() => fill(prompt)}>
            {prompt}
          </button>
        </li>
      ))}
    </ul>
  );
}
