import type { Sentence } from "@/plan/api/schema-concept";
import type {
  CoverRef,
  Gap,
  NodeRef,
  SettleState,
  TraceDoc,
  TraceHeading,
  TraceView,
} from "@/plan/api/schema-trace";
import { featureSourceHref } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";
import { plural } from "@/plan/ui/plural";

// Pure reads of a feature's trace for the browser: labels, links, edge chips and turn impact.
// Nothing here fetches or renders; `state.ts` holds the trace and the components draw these.

/** The most hints the spine shows (the server already caps the frontier at five). */
export const FRONTIER_CAP = 5;

/** Which kind of page a summary sentence sits on: chips read different edges on each. */
export type PageKind = "product" | "plan";

const SETTLE_LABELS: Record<SettleState, string> = {
  draft: "Draft",
  revised: "Revised",
  stub_changed: "Stub changed",
  upstream_changed: "Upstream changed",
  settled: "Settled",
};

/** The word for a settle state, as the spine, headings and turn footer read it. */
export function settleLabel(state: SettleState): string {
  return SETTLE_LABELS[state];
}

/** The states that mean a settled document moved since the reader settled it. */
const MOVED: readonly SettleState[] = ["revised", "stub_changed", "upstream_changed"];

/** The trace's document at a repository path (`doc/product/<f>/section_01.md`). */
export function docOfPath(trace: TraceView, repoPath: string): TraceDoc | undefined {
  return trace.documents.find((doc) => doc.path === repoPath);
}

/**
 * The settle state a screen shows for a document: null for one the reader is not asked to settle
 * (a stub, or done work, which the server marks `settleable: false`). Every mark, control, note
 * and count of the settle flow reads the state through this.
 */
export function shownSettle(doc: TraceDoc | undefined): SettleState | null {
  return doc?.settleable ? doc.settle : null;
}

function baseName(path: string): string {
  return path.split("/").at(-1) ?? path;
}

/**
 * The summary route of a document. A plan or section that is only a stub has no page, so it
 * resolves to the place that holds its stub: the metaplan or the vision.
 */
export function summaryHref(feature: string, doc: TraceDoc): string {
  if (doc.role === "vision") return featureHref(feature, "vision");
  if (doc.role === "metaplan") return featureHref(feature, "metaplan");
  if (doc.role === "plan") {
    return doc.slug !== null && doc.progress !== "stub"
      ? featureHref(feature, `engineering/${encodeURIComponent(doc.slug)}`)
      : featureHref(feature, "metaplan");
  }
  const file = /section_([^/]+)\.md$/.exec(doc.path)?.[1];
  return file !== undefined && doc.progress !== "stub"
    ? featureHref(feature, `product/section_${file}`)
    : featureHref(feature, "vision");
}

/** The headings a node names: one by anchor, or every heading of the section when it has none. */
function headingsOf(trace: TraceView, ref: { path: string; anchor: string | null }) {
  return trace.headings.filter(
    (heading) => heading.path === ref.path && (!ref.anchor || heading.anchor === ref.anchor),
  );
}

// ---- Frontier ------------------------------------------------------------------------------

export interface FrontierLink {
  text: string;
  to: string;
}

function entryAt(trace: TraceView, node: NodeRef) {
  return node.anchor
    ? trace.entries.find((entry) => entry.anchor === node.anchor)
    : trace.entries.find((entry) => entry.plan === node.path);
}

/** "Revisit <entry> before writing its plan", or "<plan> before running it" once it is written. */
function revisitLink(trace: TraceView, feature: string, gap: Gap): FrontierLink {
  const doc = docOfPath(trace, gap.node.path);
  const entry = entryAt(trace, gap.node);
  const unwritten = doc?.role === "plan" ? doc.progress === "stub" : entry?.plan === null;
  if (unwritten) {
    const anchor = entry?.anchor ?? gap.node.anchor;
    return {
      text: `Revisit ${entry?.title ?? doc?.title ?? baseName(gap.node.path)} before writing its plan`,
      to: `${featureHref(feature, "metaplan")}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`,
    };
  }
  // A gap on a metaplan entry names the entry's written plan.
  const plan = doc?.role === "plan" || !entry?.plan ? doc : docOfPath(trace, entry.plan);
  const title = plan?.title ?? entry?.title ?? baseName(gap.node.path);
  const to = plan ? summaryHref(feature, plan) : featureHref(feature, "metaplan");
  return { text: `Revisit ${title} before running it`, to };
}

/** The sentence a document gap reads as, keyed by the gap kind. */
const DOC_GAP_TEXT: Partial<Record<Gap["kind"], (title: string) => string>> = {
  revised: (title) => `${title} changed since you settled it`,
  stub_changed: (title) => `${title}'s stub changed since you settled it`,
  upstream_changed: (title) => `The spec under ${title} changed since you settled it`,
  settle: (title) => `Read ${title} and settle it`,
};

function frontierLink(trace: TraceView, feature: string, gap: Gap): FrontierLink {
  if (gap.kind === "revisit") return revisitLink(trace, feature, gap);
  if (gap.kind === "triage") {
    const count = gap.count ?? 0;
    return {
      text: plural(count, "open finding"),
      to: featureHref(feature, "findings"),
    };
  }
  if (gap.kind === "uncovered") {
    const heading = headingsOf(trace, gap.node)[0];
    return {
      text: `${heading?.title ?? gap.node.anchor ?? baseName(gap.node.path)} is not covered by any plan`,
      to: featureSourceHref(feature, gap.node.path, { anchor: gap.node.anchor }),
    };
  }
  const doc = docOfPath(trace, gap.node.path);
  const title = doc?.title ?? baseName(gap.node.path);
  return {
    text: DOC_GAP_TEXT[gap.kind]!(title),
    to: doc ? summaryHref(feature, doc) : featureSourceHref(feature, gap.node.path),
  };
}

/** The frontier as at most five hints, one link each, in the server's order. */
export function frontierLinks(trace: TraceView, feature: string): FrontierLink[] {
  return trace.frontier.slice(0, FRONTIER_CAP).map((gap) => frontierLink(trace, feature, gap));
}

// ---- Brief bundle --------------------------------------------------------------------------

export interface BundleLink {
  path: string;
  /** The brief's path under `briefs/<slug>/` (`build/r1.md`). */
  label: string;
  to: string;
}

function bundleLabel(path: string): string {
  const [, rest = path] = path.split("/briefs/");
  const [, ...inside] = rest.split("/");
  return inside.length > 0 ? inside.join("/") : baseName(path);
}

/** A plan's brief files, each linked to its source page; none for a document that is no plan. */
export function bundleLinks(trace: TraceView, planPath: string, feature: string): BundleLink[] {
  const doc = docOfPath(trace, planPath);
  return (doc?.bundle ?? []).map((path) => ({
    path,
    label: bundleLabel(path),
    to: featureSourceHref(feature, path),
  }));
}

// ---- Edge chips ----------------------------------------------------------------------------

export interface EdgeChip {
  key: string;
  /** `entry`, `plan` and `stage` cover a spec heading; `spec` is a heading a plan implements. */
  kind: "entry" | "plan" | "stage" | "spec" | "findings";
  label: string;
  /** The document (and heading) the chip opens; null for the findings count. */
  path: string | null;
  anchor: string | null;
  /** A spec chip's section settle state; null for every other chip. */
  state: SettleState | null;
  /** The findings chip's open count; null for every other chip. */
  count: number | null;
}

type ChipSentence = Pick<Sentence, "rests_on" | "citations">;

function coverChip(trace: TraceView, cover: CoverRef): EdgeChip {
  const slug = cover.slug ?? baseName(cover.path);
  const label =
    cover.kind === "entry"
      ? (trace.entries.find((entry) => entry.anchor === cover.anchor)?.title ?? slug)
      : cover.kind === "stage"
        ? `${slug} · ${cover.stage ?? cover.anchor ?? ""}`
        : slug;
  return {
    key: `${cover.kind}:${cover.path}#${cover.anchor ?? ""}`,
    kind: cover.kind,
    label,
    path: cover.path,
    anchor: cover.anchor,
    state: null,
    count: null,
  };
}

function openFindings(trace: TraceView, headings: TraceHeading[]): number {
  const ids = new Set(headings.flatMap((heading) => heading.findings));
  return trace.findings.filter(
    ({ finding }) => ids.has(finding.id) && finding.status.kind === "open",
  ).length;
}

function productChips(trace: TraceView, sentence: ChipSentence): EdgeChip[] {
  const headings = sentence.rests_on.flatMap((ref) => headingsOf(trace, ref));
  const chips = new Map<string, EdgeChip>();
  for (const cover of headings.flatMap((heading) => heading.covered_by)) {
    const chip = coverChip(trace, cover);
    chips.set(chip.key, chip);
  }
  const open = openFindings(trace, headings);
  const list = [...chips.values()];
  if (open > 0) {
    const label = plural(open, "open finding");
    list.push({
      key: "findings",
      kind: "findings",
      label,
      path: null,
      anchor: null,
      state: null,
      count: open,
    });
  }
  return list;
}

/** The plans a plan-page sentence is about, with the stage ids it cites in each. */
function citedPlans(trace: TraceView, sentence: ChipSentence): Map<string, string[]> {
  const plans = new Map<string, string[]>();
  const add = (path: string) => {
    if (docOfPath(trace, path)?.role === "plan" && !plans.has(path)) plans.set(path, []);
  };
  sentence.rests_on.forEach((ref) => add(ref.path));
  for (const citation of sentence.citations) {
    if (citation.kind !== "source") continue;
    add(citation.path);
    if (citation.anchor.startsWith("stage:")) {
      plans.get(citation.path)?.push(citation.anchor.slice("stage:".length));
    }
  }
  return plans;
}

/** Whether a heading's cover is what the plan, or only its cited stages, implements. */
function implementedBy(cover: CoverRef, plan: string, stages: string[]): boolean {
  if (cover.path !== plan || cover.kind === "entry") return false;
  return stages.length === 0 || (cover.kind === "stage" && stages.includes(cover.stage ?? ""));
}

function planChips(trace: TraceView, sentence: ChipSentence): EdgeChip[] {
  const chips = new Map<string, EdgeChip>();
  for (const [plan, stages] of citedPlans(trace, sentence)) {
    for (const heading of trace.headings) {
      if (!heading.covered_by.some((cover) => implementedBy(cover, plan, stages))) continue;
      const key = `spec:${heading.path}#${heading.anchor}`;
      chips.set(key, {
        key,
        kind: "spec",
        label: heading.title,
        path: heading.path,
        anchor: heading.anchor,
        state: shownSettle(docOfPath(trace, heading.path)),
        count: null,
      });
    }
  }
  return [...chips.values()];
}

/**
 * The chips a summary sentence shows. On a product page: the entries, plans and stages covering
 * the headings it rests on (an empty anchor means the whole section), then the open findings on
 * them. On a plan page: the spec headings the cited stage (`stage:<id>` anchor) or the plan
 * implements, each with its section's settle state.
 */
export function chipsFor(trace: TraceView, sentence: ChipSentence, pageKind: PageKind): EdgeChip[] {
  return pageKind === "product" ? productChips(trace, sentence) : planChips(trace, sentence);
}

// ---- Turn impact ---------------------------------------------------------------------------

export interface ImpactRow {
  path: string;
  title: string;
  /** Null for a document the reader is not asked to settle (a done plan covering the section). */
  state: SettleState | null;
}

export interface ImpactGroup {
  /** The changed document these rows follow from. */
  path: string;
  rows: ImpactRow[];
}

function coveringPlans(trace: TraceView, section: string): TraceDoc[] {
  const plans = trace.headings
    .filter((heading) => heading.path === section)
    .flatMap((heading) => heading.covered_by)
    .filter((cover) => cover.kind !== "entry")
    .map((cover) => docOfPath(trace, cover.path));
  return plans.filter((doc): doc is TraceDoc => doc !== undefined);
}

function coveredSections(trace: TraceView, plan: string): TraceDoc[] {
  const sections = trace.headings
    .filter((heading) => heading.covered_by.some((cover) => cover.path === plan))
    .map((heading) => docOfPath(trace, heading.path));
  return sections.filter((doc): doc is TraceDoc => doc !== undefined);
}

/** Documents tied to `path` by a stub, a cover or a brief bundle, other than plans covering it. */
function relatedDocs(trace: TraceView, path: string): TraceDoc[] {
  const doc = docOfPath(trace, path);
  const stubOwner = doc?.stub ? docOfPath(trace, doc.stub.path) : undefined;
  return [
    ...(stubOwner ? [stubOwner] : []),
    ...trace.documents.filter((other) => other.stub?.path === path || other.bundle.includes(path)),
    ...(doc?.role === "plan" ? coveredSections(trace, path) : []),
  ];
}

function impactOf(trace: TraceView, path: string): ImpactGroup {
  const rows = new Map<string, ImpactRow>();
  const add = (doc: TraceDoc) => {
    if (doc.path !== path)
      rows.set(doc.path, { path: doc.path, title: doc.title, state: shownSettle(doc) });
  };
  relatedDocs(trace, path)
    .filter((doc) => {
      const state = shownSettle(doc);
      return state !== null && MOVED.includes(state);
    })
    .forEach(add);
  // A plan covering a changed section is listed in whatever state it is in: the reader decides.
  if (docOfPath(trace, path)?.role === "section") coveringPlans(trace, path).forEach(add);
  return { path, rows: [...rows.values()] };
}

/**
 * For each changed document, the related documents that are now `revised`, `stub_changed` or
 * `upstream_changed`, plus the plans covering a changed section, with their states.
 */
export function impactFor(trace: TraceView, changedPaths: string[]): ImpactGroup[] {
  return changedPaths.map((path) => impactOf(trace, path));
}
