import type { KeyboardEvent } from "react";

import { isTyping } from "@/lib/is-typing";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { toAddress } from "@/plan/files/address";
import type { Store } from "@/state/store";

import { sentenceCitations, targetKey, type SentenceSite } from "./column-context";
import { findSite } from "./column-model";
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
  return (
    target.kind === "sentence" &&
    env.open.some(
      (run) => run.sentenceId === target.id && run.parent === target.runOf && run.level < 2,
    )
  );
}

/** The innermost run around the focus: the sentence's own when open, else the run holding it. */
function runAround(env: KeyEnv, target: ColumnTarget): OpenRun | undefined {
  if (isOpenAt(env, target)) {
    return env.open.find((run) => run.sentenceId === target.id && run.parent === target.runOf);
  }
  if (target.runOf === null) return undefined;
  // A cited sentence's run is a page sentence's (parent null); a passage names its run's parent.
  const parent = target.kind === "passage" ? (target.parent ?? null) : null;
  return env.open.find(
    (run) => run.sentenceId === target.runOf && run.parent === parent && run.level < 2,
  );
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
  if (source) env.openAddress(toAddress(source.path, source.anchor));
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
  if (page) env.openAddress(`/${page}`);
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
  onAttach({
    kind: "summary",
    summary: site.summary,
    sentence: site.sentence.id,
    text: site.sentence.text,
    restsOn: site.sentence.rests_on,
    hash: summaryPage?.file_hash ?? env.page.file_hash,
  });
}

function step(env: KeyEnv, index: number, delta: number): void {
  const next = env.order[Math.min(Math.max(index + delta, 0), env.order.length - 1)];
  if (next) env.move(targetKey(next));
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
  if (event.shiftKey && event.key === "Escape") {
    event.preventDefault();
    env.store.set(openRunsAtom(env.pageKey), []);
    return;
  }
  const run =
    runAround(env, target) ?? (event.key === "Escape" ? env.open[env.open.length - 1] : undefined);
  if (run) {
    event.preventDefault();
    env.foldRun(run);
  }
}

/**
 * The reading column's keys (section 05, "Keyboard"). Keys are ignored while the user types
 * and when they come from a control inside the column (a link in a passage keeps Enter).
 * Escape folds the innermost run around focus, else the last opened; Shift+Escape folds all.
 * ArrowLeft folds only the innermost run around focus.
 * `g p` and `g e` live in `useGoKeys` (concept-route.tsx), active only on concept routes.
 */
export function handleColumnKey(
  event: KeyboardEvent<HTMLElement>,
  focusKey: string | null,
  env: KeyEnv,
): void {
  if (event.ctrlKey || event.metaKey || event.altKey || isTyping(event.target)) return;
  const element = event.target as HTMLElement;
  if (element !== event.currentTarget && !element.hasAttribute("data-focus")) return;
  const key = element.getAttribute("data-focus") ?? focusKey;
  const index = env.order.findIndex((target) => targetKey(target) === key);
  const target = env.order[index];
  if (!target) return;
  const consume = () => event.preventDefault();
  switch (event.key) {
    case "ArrowDown":
    case "j":
      consume();
      return step(env, index, 1);
    case "ArrowUp":
    case "k":
      consume();
      return step(env, index, -1);
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
