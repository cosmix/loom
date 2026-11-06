import type { KeyboardEvent } from "react";

import { isTyping } from "@/lib/is-typing";
import type { CoverageModel } from "@/plan/trace/coverage-model";

// Keys of the coverage map. `j`/`k` walk a column, `l` goes to the first downstream card of the
// focused card's lineage (how), `h` to the first upstream one (why), `Enter` opens the node.
// Like the reading column's keys they stay out of the way while the reader types or a dialog is
// open (`concept-route.tsx`'s Escape handling decides the same way).

/** The attribute the page puts on each card (`data-key`); the handler reads the focused card from it. */
const CARD_KEY = "[data-key]";

/** An open Radix dialog or an alert dialog; the same test the reading column uses for Escape. */
export const OPEN_DIALOG = '[role="dialog"][data-state="open"], [role="alertdialog"]';

export interface KeyContext {
  model: CoverageModel;
  /** The keys the current filter shows; keys never land on a hidden card. */
  visible: Set<string>;
  /** Selects `key` and moves focus to its card. */
  focus: (key: string) => void;
  /** Opens the node a card stands for. */
  open: (href: string) => void;
}

/** The card a movement key leads to from `from`, null when there is nowhere to go. */
export function moveTarget(
  model: CoverageModel,
  visible: Set<string>,
  from: string,
  key: string,
): string | null {
  const here = model.byKey.get(from);
  if (!here) return null;
  const shown = (keys: string[] | undefined) => (keys ?? []).filter((next) => visible.has(next));
  if (key === "l") return shown(model.down.get(from))[0] ?? null;
  if (key === "h") return shown(model.up.get(from))[0] ?? null;
  if (key !== "j" && key !== "k") return null;
  const column = model.cards.filter((card) => card.column === here.column && visible.has(card.key));
  const at = column.findIndex((card) => card.key === from);
  return column[at + (key === "j" ? 1 : -1)]?.key ?? null;
}

/** The key of the card the event came from, or null when the keys are off for this event. */
function activeCard(event: KeyboardEvent): string | null {
  if (event.altKey || event.ctrlKey || event.metaKey || event.defaultPrevented) return null;
  if (isTyping(event.target) || document.querySelector(OPEN_DIALOG)) return null;
  const target = event.target instanceof Element ? event.target : null;
  return target?.closest(CARD_KEY)?.getAttribute("data-key") ?? null;
}

/** Handles one key press on the map; keys it does not own are left to the page and the browser. */
export function handleCoverageKey(event: KeyboardEvent, context: KeyContext): void {
  if (!["j", "k", "h", "l", "Enter"].includes(event.key)) return;
  const from = activeCard(event);
  if (from === null) return;
  event.preventDefault();
  if (event.key === "Enter") {
    const href = context.model.byKey.get(from)?.href;
    if (href !== undefined) context.open(href);
    return;
  }
  const to = moveTarget(context.model, context.visible, from, event.key);
  if (to !== null) context.focus(to);
}
