import type { JSX } from "react";
import type { AttachmentDraft } from "./types";

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
