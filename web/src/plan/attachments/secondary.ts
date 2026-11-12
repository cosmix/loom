import type { JSX } from "react";
import type { AttachmentDraft } from "./types";

/**
 * One extra action beside "Add to chat", or null when it has nothing to offer for this selection
 * (the pill then draws no rule or frame for it). AddToChat calls it while it renders, so it may
 * read atoms with hooks; the set of registered secondaries must not change while a selection
 * control is shown, which holds because registration follows the chat panel's mount.
 */
export type SelectionSecondary = (draft: AttachmentDraft) => JSX.Element | null;

const registrations: { render: SelectionSecondary }[] = [];

export function registerSelectionSecondary(render: SelectionSecondary): () => void {
  const registration = { render };
  registrations.push(registration);
  return () => {
    const index = registrations.indexOf(registration);
    if (index !== -1) registrations.splice(index, 1);
  };
}

export function selectionSecondaries(): SelectionSecondary[] {
  return registrations.map(({ render }) => render);
}
