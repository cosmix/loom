import type { ConceptPage, Sentence, SourceAnchor } from "@/plan/api/schema-concept";
import type { FoldNote, OpenRun, SourceAnchorRef } from "./types";

export type { FoldNote, OpenRun, SourceAnchorRef } from "./types";

/**
 * Resolves a sentence citation (`{ summary, id }`) to the cited sentence of another loaded
 * page. An overview's level-1 sentences live on the summary pages it cites, so carrying a
 * nested level-1 run needs their `rests_on`; without a lookup the run keeps its id when the
 * parent still cites it.
 */
export type CitedLookup = (summary: string, id: string) => Sentence | undefined;

export type CarryResult = { open: OpenRun[]; notes: FoldNote[] };

type Candidate = { id: string; restsOn: SourceAnchor[] };

type Context = {
  prev: Sentence[];
  top: Sentence[];
  topIndex: Map<string, number>;
  anchors: Set<string>;
  open: OpenRun[];
  cited: CitedLookup | undefined;
  fallbackPath: string;
  results: Map<number, OpenRun | null>;
  notes: FoldNote[];
  /** `paragraphOf|anchor` of every removed-section note, so a run and its passage note once. */
  noted: Set<string>;
};

/** The page's own sentences in document order (the sentences its paragraphs hold). */
export function pageSentences(page: ConceptPage): Sentence[] {
  return page.blocks.flatMap((block) => (block.kind === "paragraph" ? block.sentences : []));
}

export function anchorKey(anchor: SourceAnchor): string {
  return `${anchor.path}#${anchor.anchor}`;
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

function titleOf(anchor: SourceAnchorRef): string {
  return anchor.title ?? anchor.anchor;
}

/** "Section Port was removed from section_02.md" (section 05, "What stays open"). */
export function removedSectionNote(anchor: SourceAnchorRef, paragraphOf: string): FoldNote {
  const text = `Section ${titleOf(anchor)} was removed from ${fileName(anchor.path)}`;
  return { paragraphOf, text, path: anchor.path };
}

/** "This summary was rewritten and no longer draws on Port", named for the first anchor. */
export function rewrittenNote(
  anchor: SourceAnchorRef | undefined,
  paragraphOf: string,
  fallbackPath: string,
): FoldNote {
  if (!anchor) return { paragraphOf, text: "This summary was rewritten", path: fallbackPath };
  const text = `This summary was rewritten and no longer draws on ${titleOf(anchor)}`;
  return { paragraphOf, text, path: anchor.path };
}

function keysOf(anchors: SourceAnchor[]): Set<string> {
  return new Set(anchors.map(anchorKey));
}

function sameAnchors(anchors: SourceAnchor[], wanted: Set<string>): boolean {
  const keys = keysOf(anchors);
  return keys.size === wanted.size && [...keys].every((key) => wanted.has(key));
}

/**
 * The candidate with the largest overlap of source anchors, first in reading order on a tie,
 * none at zero overlap. A candidate with the run's own id and exactly its anchors is the
 * same sentence unchanged (a mark-only update) and keeps the run in place.
 */
function pick(run: OpenRun, candidates: Candidate[]): Candidate | null {
  const wanted = keysOf(run.restsOn);
  const same = candidates.find((candidate) => candidate.id === run.sentenceId);
  if (same && sameAnchors(same.restsOn, wanted)) return same;
  let best: Candidate | null = null;
  let bestScore = 0;
  for (const candidate of candidates) {
    const score = [...keysOf(candidate.restsOn)].filter((key) => wanted.has(key)).length;
    if (score > bestScore) {
      best = candidate;
      bestScore = score;
    }
  }
  return best;
}

/** New anchors, keeping each one's last known title from the old run. */
function withTitles(anchors: SourceAnchor[], old: SourceAnchorRef[]): SourceAnchorRef[] {
  const titles = new Map(
    old.flatMap((anchor): [string, string][] =>
      anchor.title ? [[anchorKey(anchor), anchor.title]] : [],
    ),
  );
  return anchors.map(({ path, anchor }) => {
    const title = titles.get(anchorKey({ path, anchor }));
    return title === undefined ? { path, anchor } : { path, anchor, title };
  });
}

/**
 * Where a folded top-level run's note sits: the nearest sentence at or before the old one
 * (in the previous page's order) that is still on the next page, or "" for the page start.
 */
function topParagraph(ctx: Context, run: OpenRun): string {
  const index = ctx.prev.findIndex((sentence) => sentence.id === run.sentenceId);
  for (let at = index; at >= 0; at -= 1) {
    const id = ctx.prev[at]!.id;
    if (ctx.topIndex.has(id)) return id;
  }
  return "";
}

/** "Section X was removed" once per paragraph and section. */
function noteRemoved(ctx: Context, anchor: SourceAnchorRef, paragraphOf: string): void {
  const key = `${paragraphOf}|${anchorKey(anchor)}`;
  if (ctx.noted.has(key)) return;
  ctx.noted.add(key);
  ctx.notes.push(removedSectionNote(anchor, paragraphOf));
}

/** Last known section titles from the run's own open passage entries, by anchor key. */
function passageTitles(ctx: Context, run: OpenRun): Map<string, string> {
  return new Map(
    ctx.open.flatMap((entry): [string, string][] => {
      const anchor = entry.restsOn[0];
      const child = entry.level === 2 && entry.parent === run.sentenceId;
      return child && anchor?.title ? [[anchorKey(anchor), anchor.title]] : [];
    }),
  );
}

/**
 * The passages an open sentence shows are derived from its `rests_on` (the column writes only
 * level 0 and 1 runs), so the carried run notes each section it rested on that is gone from
 * its new sentence and from every sentence of the next page: a deleted section. A section
 * that only moved to another sentence is still there and gets no note.
 */
function noteDroppedSections(
  ctx: Context,
  run: OpenRun,
  restsOn: SourceAnchor[],
  paragraphOf: string,
): void {
  const kept = keysOf(restsOn);
  const titles = passageTitles(ctx, run);
  for (const anchor of run.restsOn) {
    const key = anchorKey(anchor);
    if (kept.has(key) || ctx.anchors.has(key)) continue;
    const title = anchor.title ?? titles.get(key);
    noteRemoved(ctx, title === undefined ? anchor : { ...anchor, title }, paragraphOf);
  }
}

/** A level-0 sentence citing other sentences shows those, not passages. */
function showsPassages(ctx: Context, run: OpenRun, id: string): boolean {
  if (run.level === 1) return true;
  const index = ctx.topIndex.get(id);
  const sentence = index === undefined ? undefined : ctx.top[index];
  return !sentence?.citations.some((citation) => citation.kind === "sentence");
}

function carryTop(ctx: Context, run: OpenRun): OpenRun | null {
  if (run.level === 2) return carryPassage(ctx, run, null);
  const candidates = ctx.top.map((sentence) => ({ id: sentence.id, restsOn: sentence.rests_on }));
  const picked = pick(run, candidates);
  if (!picked) {
    ctx.notes.push(rewrittenNote(run.restsOn[0], topParagraph(ctx, run), ctx.fallbackPath));
    return null;
  }
  if (showsPassages(ctx, run, picked.id)) noteDroppedSections(ctx, run, picked.restsOn, picked.id);
  const restsOn = withTitles(picked.restsOn, run.restsOn);
  return { level: run.level, sentenceId: picked.id, restsOn, parent: null };
}

/** A level-1 run inside a level-0 run: candidates are the cited sentences of the new parent. */
function carryNested(ctx: Context, run: OpenRun, parent: OpenRun): OpenRun | null {
  const index = ctx.topIndex.get(parent.sentenceId);
  const parentSentence = index === undefined ? undefined : ctx.top[index];
  const candidates = (parentSentence?.citations ?? []).flatMap((citation) => {
    if (citation.kind !== "sentence") return [];
    const cited = ctx.cited?.(citation.summary, citation.id);
    const fallback = citation.id === run.sentenceId ? run.restsOn : [];
    return [{ id: citation.id, restsOn: cited?.rests_on ?? fallback }];
  });
  const picked = pick(run, candidates);
  if (!picked) {
    ctx.notes.push(rewrittenNote(run.restsOn[0], parent.sentenceId, ctx.fallbackPath));
    return null;
  }
  noteDroppedSections(ctx, run, picked.restsOn, parent.sentenceId);
  const restsOn = withTitles(picked.restsOn, run.restsOn);
  return { level: 1, sentenceId: picked.id, restsOn, parent: parent.sentenceId };
}

function passageAnchor(run: OpenRun): SourceAnchorRef | null {
  if (run.restsOn[0]) return run.restsOn[0];
  const split = run.sentenceId.lastIndexOf("#");
  if (split < 0) return null;
  return { path: run.sentenceId.slice(0, split), anchor: run.sentenceId.slice(split + 1) };
}

/**
 * A level-2 entry (stored data or a hand-built run; the column derives passages from the open
 * sentence instead) stays while its source section still exists on the next page. Its note
 * sits after the top-level sentence and is the one its parent run already wrote, if any.
 */
function carryPassage(ctx: Context, run: OpenRun, parent: OpenRun | null): OpenRun | null {
  const anchor = passageAnchor(run);
  if (!anchor) return null;
  const key = anchorKey(anchor);
  const exists =
    ctx.anchors.has(key) || (parent?.restsOn ?? []).some((at) => anchorKey(at) === key);
  if (!exists) {
    noteRemoved(ctx, anchor, parent ? (parent.parent ?? parent.sentenceId) : "");
    return null;
  }
  return {
    level: 2,
    sentenceId: run.sentenceId,
    restsOn: [anchor],
    parent: parent?.sentenceId ?? null,
  };
}

function carryAt(ctx: Context, index: number): OpenRun | null {
  const done = ctx.results.get(index);
  if (done !== undefined) return done;
  ctx.results.set(index, null);
  const run = ctx.open[index]!;
  let result: OpenRun | null;
  if (run.parent === null) {
    result = carryTop(ctx, run);
  } else {
    const at = ctx.open.findIndex(
      (entry) => entry.sentenceId === run.parent && entry.level < run.level,
    );
    // A parent folded away takes its children with it, under the parent's one note.
    const parent = at < 0 ? null : carryAt(ctx, at);
    if (!parent) result = null;
    else if (run.level === 2) result = carryPassage(ctx, run, parent);
    else result = carryNested(ctx, run, parent);
  }
  ctx.results.set(index, result);
  return result;
}

function childRank(ctx: Context, parent: OpenRun, child: OpenRun): number {
  let rank = -1;
  if (child.level === 2) {
    const key = anchorKey(child.restsOn[0]!);
    rank = parent.restsOn.findIndex((anchor) => anchorKey(anchor) === key);
  } else {
    const index = ctx.topIndex.get(parent.sentenceId);
    const citations = index === undefined ? [] : ctx.top[index]!.citations;
    rank = citations.findIndex(
      (citation) => citation.kind === "sentence" && citation.id === child.sentenceId,
    );
  }
  return rank < 0 ? Number.MAX_SAFE_INTEGER : rank;
}

/** Carried runs in the next page's reading order, each parent before its children. */
function ordered(ctx: Context): OpenRun[] {
  const seen = new Set<string>();
  const carried = [...ctx.results.entries()]
    .sort(([a], [b]) => a - b)
    .flatMap(([, run]) => (run ? [run] : []))
    .filter((run) => {
      const key = `${run.level}|${run.parent ?? ""}|${run.sentenceId}`;
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  const out: OpenRun[] = [];
  const emit = (run: OpenRun) => {
    out.push(run);
    carried
      .filter((child) => child.parent === run.sentenceId && child.level > run.level)
      .sort((a, b) => childRank(ctx, run, a) - childRank(ctx, run, b))
      .forEach(emit);
  };
  const topRank = (run: OpenRun) => ctx.topIndex.get(run.sentenceId) ?? Number.MAX_SAFE_INTEGER;
  carried
    .filter((run) => run.parent === null)
    .sort((a, b) => topRank(a) - topRank(b))
    .forEach(emit);
  return out;
}

/**
 * Section 05 "What stays open": carries the open runs of `prev` onto `next`, keyed to the
 * source sections each run rests on. Folded runs leave a note whose `paragraphOf` is the
 * id of the sentence on `next` the note follows (the run's restored parent), or "" for the
 * start of the page.
 */
export function carryOpenState(
  prev: ConceptPage,
  next: ConceptPage,
  open: OpenRun[],
  cited?: CitedLookup,
): CarryResult {
  const top = pageSentences(next);
  const ctx: Context = {
    prev: pageSentences(prev),
    top,
    topIndex: new Map(top.map((sentence, index) => [sentence.id, index])),
    anchors: new Set(top.flatMap((sentence) => sentence.rests_on.map(anchorKey))),
    open,
    cited,
    fallbackPath: next.path ?? "",
    results: new Map(),
    notes: [],
    noted: new Set(),
  };
  open.forEach((_, index) => carryAt(ctx, index));
  return { open: ordered(ctx), notes: ctx.notes };
}

/** A page holding one sentence per stored run, resting on the run's stored anchors. */
export function syntheticPrevious(stored: OpenRun[]): ConceptPage {
  const sentences: Sentence[] = stored.map((run) => {
    const restsOn = run.restsOn.map(({ path, anchor }) => ({ path, anchor }));
    const citations = restsOn.map((at) => ({ kind: "source" as const, ...at }));
    return { id: run.sentenceId, text: "", marks: [], citations, rests_on: restsOn, plans: [] };
  });
  return {
    key: "",
    title: "",
    path: null,
    file_hash: null,
    colour: null,
    state: null,
    plan_state: null,
    last_pressure: null,
    key_chips: [],
    blocks: [{ kind: "paragraph", sentences }],
    unlisted: [],
    implements: null,
    implemented_by: [],
    empty: null,
    building: null,
    estimate: null,
  };
}

/**
 * Stored runs read back after a reload: when a top-level run's sentence is gone from the
 * loaded page (the summary was rebuilt meanwhile), carry them as if from a page holding
 * only the stored runs; null when every stored top-level run is still on the page.
 */
export function carryStoredRuns(
  page: ConceptPage,
  stored: OpenRun[],
  cited?: CitedLookup,
): CarryResult | null {
  const ids = new Set(pageSentences(page).map((sentence) => sentence.id));
  const stale = stored.some(
    (run) => run.parent === null && run.level < 2 && !ids.has(run.sentenceId),
  );
  return stale ? carryOpenState(syntheticPrevious(stored), page, stored, cited) : null;
}
