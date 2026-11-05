import type { KeyboardEvent } from "react";

import { isTyping } from "@/lib/is-typing";
import { pageHref } from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { sourceHref } from "@/plan/feature/source-href";
import type { Store } from "@/state/store";

import { sentenceCitations, targetKey, type SentenceSite } from "./column-context";
import { focusElement } from "./column-focus";
import { findSite } from "./column-model";
import { fragmentMembers, runAt } from "./fragments";
import { pageSentences, type CitedLookup } from "./open-state";
import { pageKeyOfSummary } from "./page-state";
import { docViewAtom, passageOf } from "./passages";
import type { ColumnTarget, OpenRun } from "./types";
import { openRunsAtom } from "./view-state";

export type KeyEnv = {
  page: ConceptPage;
  pageKey: string;
  lookup: CitedLookup;
  citedPage: (summary: string) => ConceptPage | null;
  open: OpenRun[];
  order: ColumnTarget[];
  store: Store;
  move: (key: string) => void;
  openSentence: (site: SentenceSite) => void;
  foldRun: (run: OpenRun) => void;
  openAddress: (address: string) => void;
  onAttach?: (draft: AttachmentDraft) => void;
};

/** A sentence at its site; a passage's site is the sentence whose run shows it. */
function siteOfTarget(env: KeyEnv, target: ColumnTarget): SentenceSite | null {
  return target.kind === "sentence"
    ? findSite(env.page, env.lookup, env.pageKey, target.id, target.runOf)
    : findSite(env.page, env.lookup, env.pageKey, target.runOf ?? "", target.parent ?? null);
}

function isOpenAt(env: KeyEnv, target: ColumnTarget): boolean {
  if (target.kind !== "sentence") return false;
  const site = siteOfTarget(env, target);
  return site !== null && runAt(env.open, site) !== undefined;
}

/** The innermost run around the focus: the sentence's own when open, else the run holding it. */
function runAround(env: KeyEnv, target: ColumnTarget): OpenRun | undefined {
  const own = target.kind === "sentence" ? siteOfTarget(env, target) : null;
  const ownRun = own && runAt(env.open, own);
  if (ownRun) return ownRun;
  if (target.runOf === null) return undefined;
  // A cited sentence's run is a page sentence's (parent null); a passage names its run's parent.
  const parent = target.kind === "passage" ? (target.parent ?? null) : null;
  const holder = findSite(env.page, env.lookup, env.pageKey, target.runOf, parent);
  return holder ? runAt(env.open, holder) : undefined;
}

function firstSource(env: KeyEnv, target: ColumnTarget): { path: string; anchor: string } | null {
  if (target.kind === "passage") {
    const split = target.id.indexOf("#");
    return split < 0
      ? null
      : { path: target.id.slice(0, split), anchor: target.id.slice(split + 1) };
  }
  const site = siteOfTarget(env, target);
  if (!site) return null;
  const own = site.sentence.rests_on[0];
  if (own) return own;
  const first = sentenceCitations(site.sentence)[0];
  return (first && env.lookup(first.summary, first.id)?.rests_on[0]) || null;
}

function openFile(env: KeyEnv, target: ColumnTarget): void {
  const source = firstSource(env, target);
  if (source) env.openAddress(sourceHref(source.path, { anchor: source.anchor }));
}

/**
 * The summary page of an open sentence's first document: its own first mark, else the mark
 * its citer supplies for its summary, else the page of its first cited summary.
 */
function firstDocumentPage(env: KeyEnv, target: ColumnTarget): string | null {
  const site = siteOfTarget(env, target);
  if (!site) return null;
  const citing =
    site.parent === null ? undefined : pageSentences(env.page).find((s) => s.id === site.parent);
  const mark = site.sentence.marks[0] ?? citing?.marks.find((item) => item.doc.id === site.summary);
  if (mark) return mark.doc.page;
  const first = sentenceCitations(site.sentence)[0];
  return first ? pageKeyOfSummary(env.page, first.summary) : null;
}

function openFirstDocument(env: KeyEnv, target: ColumnTarget, element: HTMLElement): void {
  if (target.kind !== "sentence") return;
  // A closed sentence's own marks sit beside it and are the truth on screen (a cited
  // sentence's mark is supplied by its citer). An opened run shows other sentences' marks.
  const shown = element.hasAttribute("data-sentence-id")
    ? element.parentElement?.querySelector<HTMLElement>(":scope > .concept-marks .concept-mark")
    : null;
  if (shown) {
    shown.click();
    return;
  }
  const page = firstDocumentPage(env, target);
  if (page) env.openAddress(pageHref(page));
}

/** The `a` key: summary arm for a sentence, source arm (from the DocView) for a passage. */
function attach(env: KeyEnv, target: ColumnTarget): void {
  const { onAttach } = env;
  if (!onAttach) return;
  if (target.kind === "passage") {
    const source = firstSource(env, target);
    if (!source) return;
    void env.store.get(docViewAtom(source.path)).then(
      (doc) => {
        const passage = passageOf(doc, source.anchor);
        if (!passage) return;
        onAttach({
          kind: "source",
          path: source.path,
          anchor: source.anchor,
          text: passage.markdown,
          lines: { start: passage.startLine, end: passage.endLine },
          stageId: null,
          hash: doc.hash,
        });
      },
      // The file could not be read (gone, or the request failed): there is nothing to
      // attach, and the passage already says it could not be loaded.
      () => undefined,
    );
    return;
  }
  const site = siteOfTarget(env, target);
  if (!site) return;
  const summaryPage = site.parent === null ? env.page : env.citedPage(site.summary);
  // A fragment stands for several sentences; each is attached on its own so a draft names one.
  for (const member of fragmentMembers(site.sentence)) {
    onAttach({
      kind: "summary",
      summary: site.summary,
      sentence: member.id,
      text: member.text,
      restsOn: member.rests_on,
      hash: summaryPage?.file_hash ?? env.page.file_hash,
    });
  }
}

/** What j/k and the up and down arrows stop on: openable sentences and fold caps. */
const STOPS = '[data-sentence-id][role="button"], button.concept-run-cap';
const CAP = "button.concept-run-cap";
const RUN = '[role="group"][data-focus]';
const STEPS: Partial<Record<string, 1 | -1>> = { ArrowDown: 1, j: 1, ArrowUp: -1, k: -1 };

/**
 * Moves focus to the next (`delta` 1) or previous (-1) stop in document order, which is the
 * order on screen: a run's start cap, its sentences, its end cap. A focused element holding
 * stops (an opened run, a passage carrying its run's caps) stands on its first one, so `j`
 * from a run just opened enters it. Passage bodies and sentences that open nothing are never
 * stops; at either end the focus stays.
 */
function step(column: HTMLElement, from: HTMLElement, delta: 1 | -1): void {
  const stops = Array.from(column.querySelectorAll<HTMLElement>(STOPS));
  const origin = from === column ? from : (stops.find((stop) => from.contains(stop)) ?? from);
  const after = (stop: HTMLElement) =>
    (origin.compareDocumentPosition(stop) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
  const next =
    delta > 0
      ? stops.find((stop) => stop !== origin && after(stop))
      : stops.findLast((stop) => stop !== origin && !after(stop) && !stop.contains(origin));
  next?.focus();
}

/** The outermost run around `element` inside the column, if any. */
function outermostRun(column: HTMLElement, element: HTMLElement): HTMLElement | null {
  let outer: HTMLElement | null = null;
  for (
    let run = element.closest<HTMLElement>(RUN);
    run && column.contains(run);
    run = run.parentElement?.closest<HTMLElement>(RUN) ?? null
  ) {
    outer = run;
  }
  return outer;
}

/**
 * Shift+Escape folds every open run, from any focus in the column. Focus goes to the sentence
 * that opened the outermost run around it; asked for explicitly because the folded element
 * leaves the document (Chromium blurs it on the way out, so focus would fall to `body`).
 */
function foldAll(event: KeyboardEvent<HTMLElement>, env: KeyEnv, element: HTMLElement): void {
  event.preventDefault();
  const opener = outermostRun(event.currentTarget, element)?.getAttribute("data-focus") ?? null;
  env.store.set(openRunsAtom(env.pageKey), []);
  if (opener !== null) env.move(opener);
}

/** Opens the focused sentence; false when it is not a closed sentence that can open. */
function openFocused(env: KeyEnv, target: ColumnTarget): boolean {
  if (target.kind !== "sentence" || isOpenAt(env, target)) return false;
  const site = siteOfTarget(env, target);
  if (!site || site.sentence.citations.length === 0) return false;
  env.openSentence(site);
  return true;
}

function foldFromKey(event: KeyboardEvent<HTMLElement>, env: KeyEnv, target: ColumnTarget): void {
  const run =
    runAround(env, target) ?? (event.key === "Escape" ? env.open[env.open.length - 1] : undefined);
  if (run) {
    event.preventDefault();
    env.foldRun(run);
  }
}

/**
 * The reading column's keys (section 05, "Keyboard"). Keys are ignored while the user types.
 * Shift+Escape folds every run from anywhere in the column. On a fold cap, Escape and
 * ArrowLeft fold the cap's run (the cap's own Enter and Space do too). Other keys from a
 * control inside the column are left to it (a link in a passage keeps Enter).
 * `j`/`k` and the arrows step between openable sentences and caps (`step`).
 * Escape folds the innermost run around focus, else the last opened; ArrowLeft folds only
 * the innermost run around focus.
 * `g p` and `g e` live in `useGoKeys` (concept-route.tsx), active only on concept routes.
 */
export function handleColumnKey(
  event: KeyboardEvent<HTMLElement>,
  focusKey: string | null,
  env: KeyEnv,
): void {
  if (event.ctrlKey || event.metaKey || event.altKey || isTyping(event.target)) return;
  const element = event.target as HTMLElement;
  const column = event.currentTarget;
  if (event.key === "Escape" && event.shiftKey) return foldAll(event, env, element);
  const cap = element.matches(CAP);
  if (!cap && element !== column && !element.hasAttribute("data-focus")) return;
  const delta = STEPS[event.key];
  if (delta !== undefined) {
    event.preventDefault();
    const from = element === column ? focusElement(column, focusKey) : element;
    return step(column, from ?? column, delta);
  }
  if (cap) return foldFromCap(event, element);
  const key = element.getAttribute("data-focus") ?? focusKey;
  const target = env.order.find((item) => targetKey(item) === key);
  if (target) actOnTarget(event, env, target, element);
}

/** Escape and ArrowLeft on a cap fold the run it belongs to, as clicking it does. */
function foldFromCap(event: KeyboardEvent<HTMLElement>, cap: HTMLElement): void {
  if (event.key !== "Escape" && event.key !== "ArrowLeft") return;
  event.preventDefault();
  cap.click();
}

/** The keys that act on the focused sentence or passage. */
function actOnTarget(
  event: KeyboardEvent<HTMLElement>,
  env: KeyEnv,
  target: ColumnTarget,
  element: HTMLElement,
): void {
  const consume = () => event.preventDefault();
  switch (event.key) {
    case "Enter":
    case "ArrowRight":
      consume();
      openFocused(env, target);
      return;
    case " ":
      // A sentence button activates on Space (ARIA button pattern); elsewhere Space scrolls.
      if (openFocused(env, target)) consume();
      return;
    case "Escape":
    case "ArrowLeft":
      return foldFromKey(event, env, target);
    case "f":
      return openFile(env, target);
    case "d":
      return openFirstDocument(env, target, element);
    case "a":
      return attach(env, target);
  }
}
