import type { KeyboardEvent } from "react";

import { isTyping } from "@/lib/is-typing";
import type { ConceptPage } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { toAddress } from "@/plan/files/address";
import type { Store } from "@/state/store";

import { sentenceCitations, targetKey, type SentenceSite } from "./column-context";
import { findSite } from "./column-model";
import { docViewAtom, passageOf } from "./passages";
import type { CitedLookup } from "./open-state";
import type { FocusTarget, OpenRun } from "./types";

export type KeyEnv = {
  page: ConceptPage;
  pageKey: string;
  lookup: CitedLookup;
  citedPage: (summary: string) => ConceptPage | null;
  open: OpenRun[];
  order: FocusTarget[];
  store: Store;
  move: (key: string) => void;
  openSentence: (site: SentenceSite) => void;
  foldRun: (run: OpenRun) => void;
  openAddress: (address: string) => void;
  onAttach?: (draft: AttachmentDraft) => void;
};

function siteOfTarget(env: KeyEnv, target: FocusTarget): SentenceSite | null {
  const parent = target.kind === "sentence" ? target.runOf : null;
  if (target.kind === "sentence")
    return findSite(env.page, env.lookup, env.pageKey, target.id, parent);
  // A passage belongs to the sentence it opened from; that sentence sits at the page or in a run.
  return (
    findSite(env.page, env.lookup, env.pageKey, target.runOf ?? "", null) ??
    env.open
      .filter((run) => run.sentenceId === target.runOf && run.parent !== null)
      .map((run) => findSite(env.page, env.lookup, env.pageKey, run.sentenceId, run.parent))
      .find((site) => site !== null) ??
    null
  );
}

function isOpenAt(env: KeyEnv, target: FocusTarget): boolean {
  return (
    target.kind === "sentence" &&
    env.open.some(
      (run) => run.sentenceId === target.id && run.parent === target.runOf && run.level < 2,
    )
  );
}

/** The innermost run around the focus: the sentence's own when open, else the run holding it. */
function runAround(env: KeyEnv, target: FocusTarget): OpenRun | undefined {
  if (isOpenAt(env, target)) {
    return env.open.find((run) => run.sentenceId === target.id && run.parent === target.runOf);
  }
  if (target.runOf === null) return undefined;
  const holders = env.open.filter((run) => run.sentenceId === target.runOf && run.level < 2);
  return holders.find((run) => run.parent === null) ?? holders[0];
}

function firstSource(env: KeyEnv, target: FocusTarget): { path: string; anchor: string } | null {
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

function openFile(env: KeyEnv, target: FocusTarget): void {
  const source = firstSource(env, target);
  if (source) env.openAddress(toAddress(source.path, source.anchor));
}

function openFirstDocument(env: KeyEnv, target: FocusTarget, element: HTMLElement): void {
  if (target.kind !== "sentence") return;
  // The marks on screen are the truth (a cited sentence's mark is supplied by its citer).
  const shown = element.parentElement?.querySelector<HTMLElement>(".concept-mark");
  if (shown) {
    shown.click();
    return;
  }
  const page = siteOfTarget(env, target)?.sentence.marks[0]?.doc.page;
  if (page) env.openAddress(`/${page}`);
}

/** The `a` key: summary arm for a sentence, source arm (from the DocView) for a passage. */
function attach(env: KeyEnv, target: FocusTarget): void {
  const { onAttach } = env;
  if (!onAttach) return;
  if (target.kind === "passage") {
    const source = firstSource(env, target);
    if (!source) return;
    void env.store.get(docViewAtom(source.path)).then((doc) => {
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
    });
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

function openFocused(env: KeyEnv, target: FocusTarget): void {
  if (target.kind !== "sentence" || isOpenAt(env, target)) return;
  const site = siteOfTarget(env, target);
  if (site && site.sentence.citations.length > 0) env.openSentence(site);
}

/**
 * The reading column's keys (section 05, "Keyboard"). Keys are ignored while the user types
 * and when they come from a control inside the column (a link in a passage keeps Enter).
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
      return openFocused(env, target);
    case "Escape":
    case "ArrowLeft": {
      const run = runAround(env, target);
      if (run) {
        consume();
        env.foldRun(run);
      }
      return;
    }
    case "f":
      return openFile(env, target);
    case "d":
      return openFirstDocument(env, target, element);
    case "a":
      return attach(env, target);
  }
}
