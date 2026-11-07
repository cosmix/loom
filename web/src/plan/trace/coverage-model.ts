import type { SettleState, TraceDoc, TraceView } from "@/plan/api/schema-trace";
import { fileName } from "@/plan/concept/passages";
import { featureSourceHref } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";
import { docOfPath, settleLabel, shownSettle, summaryHref } from "@/plan/trace/model";

// The coverage map as data: five columns of cards, the lineage that ties them together and the
// filters. Pure reads of the trace; `coverage-page.tsx` draws it and `coverage-keys.ts` walks it.

export type ColumnId = "vision" | "spec" | "metaplan" | "plans" | "findings";

/** The columns left to right: why (vision, spec), then how (metaplan, plans), then findings. */
export const COLUMNS: readonly { id: ColumnId; label: string }[] = [
  { id: "vision", label: "Vision rows" },
  { id: "spec", label: "Spec headings" },
  { id: "metaplan", label: "Metaplan entries" },
  { id: "plans", label: "Plans and stages" },
  { id: "findings", label: "Findings" },
];

/** What a card needs the reader to notice; a card holds the strongest one. */
export type CardState = "plain" | "uncovered" | "open" | Exclude<SettleState, "draft" | "settled">;

export interface Card {
  key: string;
  column: ColumnId;
  title: string;
  /** A spec heading's section; null in every other column. */
  group: string | null;
  state: CardState;
  /** The words a screen reader adds after the title; null for a card with nothing to flag. */
  description: string | null;
  /** Where Enter opens the node. */
  href: string;
  /** Every card related to this one, upstream and downstream, in column order. */
  lineage: string[];
}

export type Filter = "all" | "uncovered" | "changed" | "open";

export const FILTERS: readonly { id: Filter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "uncovered", label: "Uncovered" },
  { id: "changed", label: "Changed since settled" },
  { id: "open", label: "Open findings" },
];

export interface CoverageModel {
  cards: Card[];
  byKey: Map<string, Card>;
  /** Each card's position in `cards`. */
  index: Map<string, number>;
  /** Direct neighbours, in card order: upstream explains why, downstream says how. */
  up: Map<string, string[]>;
  down: Map<string, string[]>;
}

const MOVED = new Set<CardState>(["revised", "stub_changed", "upstream_changed"]);

/**
 * The state a document's settle state gives its card; a draft or settled document, and done work
 * the reader is not asked to settle, flag nothing.
 */
function settleCardState(doc: TraceDoc | undefined): CardState {
  const state = shownSettle(doc);
  return state === null || state === "draft" || state === "settled" ? "plain" : state;
}

function describe(state: CardState): string | null {
  if (state === "plain") return null;
  if (state === "uncovered") return "Not covered";
  if (state === "open") return "Open finding";
  return `${settleLabel(state)} since settled`;
}

function card(partial: Omit<Card, "description" | "lineage" | "group"> & { group?: string }): Card {
  return {
    group: null,
    ...partial,
    description: describe(partial.state),
    lineage: [],
  };
}

function visionCards(trace: TraceView, feature: string): Card[] {
  return trace.documents
    .filter((doc) => doc.role === "section")
    .map((doc) =>
      card({
        key: `vision:${doc.path}`,
        column: "vision",
        title: doc.title,
        state: settleCardState(doc),
        href: summaryHref(feature, doc),
      }),
    );
}

function headingCards(trace: TraceView, feature: string): Card[] {
  return trace.headings.map((heading) =>
    card({
      key: `spec:${heading.path}#${heading.anchor}`,
      column: "spec",
      title: heading.title,
      group: docOfPath(trace, heading.path)?.title ?? fileName(heading.path),
      state: heading.covered_by.length === 0 ? "uncovered" : "plain",
      href: featureSourceHref(feature, heading.path, { anchor: heading.anchor }),
    }),
  );
}

function entryCards(trace: TraceView, feature: string): Card[] {
  return trace.entries.map((entry) =>
    card({
      key: `entry:${entry.anchor}`,
      column: "metaplan",
      title: entry.title,
      state: "plain",
      href: `${featureHref(feature, "metaplan")}#${encodeURIComponent(entry.anchor)}`,
    }),
  );
}

/** Every plan a heading, an entry or the document list names, in that order, with its stages. */
function planStages(trace: TraceView): Map<string, string[]> {
  const plans = new Map<string, string[]>();
  for (const doc of trace.documents) {
    if (doc.role === "plan" && doc.progress !== "stub") plans.set(doc.path, []);
  }
  for (const cover of trace.headings.flatMap((heading) => heading.covered_by)) {
    if (cover.kind === "entry") continue;
    const stages = plans.get(cover.path) ?? [];
    const stage = cover.kind === "stage" ? (cover.stage ?? cover.anchor) : null;
    if (stage !== null && !stages.includes(stage)) stages.push(stage);
    plans.set(cover.path, stages);
  }
  for (const entry of trace.entries) {
    if (entry.plan !== null && !plans.has(entry.plan)) plans.set(entry.plan, []);
  }
  return plans;
}

function planCards(trace: TraceView, feature: string): Card[] {
  const cards: Card[] = [];
  for (const [path, stages] of planStages(trace)) {
    const doc = docOfPath(trace, path);
    const name = doc?.slug ?? fileName(path);
    cards.push(
      card({
        key: `plan:${path}`,
        column: "plans",
        title: doc?.title ?? name,
        state: settleCardState(doc),
        href: doc ? summaryHref(feature, doc) : featureSourceHref(feature, path),
      }),
    );
    for (const stage of stages) {
      cards.push(
        card({
          key: `stage:${path}#${stage}`,
          column: "plans",
          title: `${name} · ${stage}`,
          state: "plain",
          href: featureSourceHref(feature, path, { anchor: `stage:${stage}` }),
        }),
      );
    }
  }
  return cards;
}

function findingCards(trace: TraceView, feature: string): Card[] {
  return trace.findings.map(({ finding }) =>
    card({
      key: `finding:${finding.id}`,
      column: "findings",
      title: `${finding.id} ${finding.title}`,
      state: finding.status.kind === "open" ? "open" : "plain",
      href: `${featureHref(feature, "findings")}#finding-${encodeURIComponent(finding.id)}`,
    }),
  );
}

type Edges = Map<string, Set<string>>;

function link(edges: Edges, from: string, to: string): void {
  const targets = edges.get(from) ?? new Set<string>();
  targets.add(to);
  edges.set(from, targets);
}

/** The edges that run from a spec heading to what covers it and what was found about it. */
function headingEdges(trace: TraceView, known: Set<string>, edges: Edges): void {
  for (const heading of trace.headings) {
    const from = `spec:${heading.path}#${heading.anchor}`;
    link(edges, `vision:${heading.path}`, from);
    for (const cover of heading.covered_by) {
      if (cover.kind === "entry") link(edges, from, `entry:${cover.anchor ?? ""}`);
      else link(edges, from, `plan:${cover.path}`);
      if (cover.kind === "stage") {
        link(edges, `plan:${cover.path}`, `stage:${cover.path}#${cover.stage ?? cover.anchor}`);
      }
    }
    for (const id of heading.findings) link(edges, from, `finding:${id}`);
  }
  for (const entry of trace.entries) {
    if (entry.plan !== null) link(edges, `entry:${entry.anchor}`, `plan:${entry.plan}`);
  }
  // A finding that names a plan or a metaplan entry hangs off it as well as off its headings.
  for (const { finding, about } of trace.findings) {
    for (const node of about) {
      const role = docOfPath(trace, node.path)?.role;
      if (role !== "plan" && role !== "metaplan") continue;
      const owner = role === "plan" ? `plan:${node.path}` : `entry:${node.anchor ?? ""}`;
      if (known.has(owner)) link(edges, owner, `finding:${finding.id}`);
    }
  }
}

function reach(edges: Map<string, string[]>, from: string): Set<string> {
  const seen = new Set<string>();
  const queue = [from];
  for (let key = queue.pop(); key !== undefined; key = queue.pop()) {
    for (const next of edges.get(key) ?? []) {
      if (seen.has(next)) continue;
      seen.add(next);
      queue.push(next);
    }
  }
  return seen;
}

/** The edges between cards that exist, as neighbour lists in card order. */
function neighbours(edges: Edges, order: Map<string, number>) {
  const up = new Map<string, string[]>();
  const down = new Map<string, string[]>();
  for (const [from, targets] of edges) {
    if (!order.has(from)) continue;
    for (const to of targets) {
      if (!order.has(to)) continue;
      down.set(from, [...(down.get(from) ?? []), to]);
      up.set(to, [...(up.get(to) ?? []), from]);
    }
  }
  const byOrder = (a: string, b: string) => order.get(a)! - order.get(b)!;
  for (const list of [...up.values(), ...down.values()]) list.sort(byOrder);
  return { up, down };
}

/** The coverage map of a feature's trace. */
export function buildCoverage(trace: TraceView, feature: string): CoverageModel {
  const cards = [
    ...visionCards(trace, feature),
    ...headingCards(trace, feature),
    ...entryCards(trace, feature),
    ...planCards(trace, feature),
    ...findingCards(trace, feature),
  ];
  const order = new Map(cards.map((entry, index) => [entry.key, index]));
  const edges: Edges = new Map();
  headingEdges(trace, new Set(order.keys()), edges);
  const { up, down } = neighbours(edges, order);
  for (const entry of cards) {
    const related = new Set([...reach(up, entry.key), ...reach(down, entry.key)]);
    entry.lineage = [...related].sort((a, b) => order.get(a)! - order.get(b)!);
  }
  const byKey = new Map(cards.map((entry) => [entry.key, entry]));
  return { cards, byKey, index: order, up, down };
}

/** The cards a card descends from (`up`, transitively) and the ones that follow from it (`down`). */
export function lineage(model: CoverageModel, key: string): { up: string[]; down: string[] } {
  const sorted = (keys: Set<string>) =>
    [...keys].sort((a, b) => model.index.get(a)! - model.index.get(b)!);
  return { up: sorted(reach(model.up, key)), down: sorted(reach(model.down, key)) };
}

function matches(entry: Card, filter: Filter): boolean {
  if (filter === "uncovered") return entry.state === "uncovered";
  if (filter === "changed") return MOVED.has(entry.state);
  return entry.state === "open";
}

/**
 * The keys a filter shows: the cards it picks out and their lineage, so a gap still shows the
 * section it sits in.
 */
export function visibleKeys(model: CoverageModel, filter: Filter): Set<string> {
  if (filter === "all") return new Set(model.cards.map((entry) => entry.key));
  const shown = new Set<string>();
  for (const entry of model.cards.filter((candidate) => matches(candidate, filter))) {
    shown.add(entry.key);
    entry.lineage.forEach((key) => shown.add(key));
  }
  return shown;
}
