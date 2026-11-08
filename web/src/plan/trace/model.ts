import type { Gap, NodeRef, SettleState, TraceDoc, TraceView } from "@/plan/api/schema-trace";
import { fileName } from "@/plan/concept/passages";
import type { GlyphKind } from "@/plan/feature/spine-model";
import { featureSourceHref } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";

// Pure reads of a feature's trace for the browser: labels, links, the spine head and turn impact.
// Nothing here fetches or renders; `state.ts` holds the trace and the components draw these.

/** The states that mean a settled document moved since the reader settled it. */
const MOVED: readonly SettleState[] = ["revised", "stub_changed", "upstream_changed"];

export function isMoved(state: SettleState): boolean {
  return MOVED.includes(state);
}

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

/**
 * A document title without the number it opens with, when that number is `number` ("04 — Summary
 * Engine" for 4 reads "Summary Engine"). A title that opens with another number, or none, stays
 * whole. Pages that draw the number beside the title use this so it is not printed twice.
 */
export function withoutNumber(title: string, number: number | string): string {
  const prefix = /^(\d+)\s*[—–:.-]?\s+/.exec(title);
  return prefix !== null && Number(prefix[1]) === Number(number)
    ? title.slice(prefix[0].length)
    : title;
}

/** Where a heading or a section sits on the Coverage page: the section file's stem, then `:anchor`. */
export function coverageHref(feature: string, path: string, anchor?: string | null): string {
  const stem = fileName(path).replace(/\.md$/, "");
  const at = anchor ? `${stem}:${anchor}` : stem;
  return `${featureHref(feature, "coverage")}#${encodeURIComponent(at)}`;
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

/** The glyph that leads the spine head: a spine row's glyph kind, or the Findings instrument's icon. */
export type HeadGlyph = Extract<GlyphKind, "not-written" | "attention" | "draft"> | "findings";

/**
 * What the spine head says: a verb and a name (a plan's short slug when `nameIsSlug`), the reason
 * it is next, an optional further muted line, the glyph that leads it and the place it links to.
 */
export interface FrontierHead {
  verb: string;
  name: string;
  nameIsSlug: boolean;
  reason: string;
  detail: string | null;
  glyph: HeadGlyph;
  to: string;
}

/** A plan's slug without its feature prefix (`feat-planner-05-trace` is `05-trace` in `feat-planner`). */
export function shortSlug(feature: string, slug: string): string {
  const prefix = `${feature}-`;
  return slug.startsWith(prefix) && slug.length > prefix.length ? slug.slice(prefix.length) : slug;
}

function entryAt(trace: TraceView, node: NodeRef) {
  return node.anchor
    ? trace.entries.find((entry) => entry.anchor === node.anchor)
    : trace.entries.find((entry) => entry.plan === node.path);
}

/** "Revisit <entry>" before its plan is written, or "Revisit <plan>" before running it once it is. */
function revisitHead(trace: TraceView, feature: string, gap: Gap): FrontierHead {
  const doc = docOfPath(trace, gap.node.path);
  const entry = entryAt(trace, gap.node);
  const unwritten = doc?.role === "plan" ? doc.progress === "stub" : entry?.plan === null;
  const base = { verb: "Revisit", detail: null, glyph: "attention" } as const;
  if (unwritten) {
    const anchor = entry?.anchor ?? gap.node.anchor;
    return {
      ...base,
      name: entry?.title ?? doc?.title ?? fileName(gap.node.path),
      nameIsSlug: false,
      reason: "Before writing its plan",
      to: `${featureHref(feature, "metaplan")}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`,
    };
  }
  // A gap on a metaplan entry names the entry's written plan.
  const plan = doc?.role === "plan" || !entry?.plan ? doc : docOfPath(trace, entry.plan);
  return {
    ...base,
    ...planName(feature, plan, plan?.title ?? entry?.title ?? fileName(gap.node.path)),
    reason: "Before running it",
    to: plan ? summaryHref(feature, plan) : featureHref(feature, "metaplan"),
  };
}

/** A plan is named by its short slug; any other document, or a plan without one, by its title. */
function planName(feature: string, doc: TraceDoc | undefined, title: string) {
  return doc?.slug
    ? { name: shortSlug(feature, doc.slug), nameIsSlug: true }
    : { name: title, nameIsSlug: false };
}

/** Counts up to twenty read as words ("three findings"); a larger count stays a number. */
const COUNT_WORDS = (
  "zero one two three four five six seven eight nine ten eleven twelve thirteen fourteen " +
  "fifteen sixteen seventeen eighteen nineteen twenty"
).split(" ");

/** "3 findings" as the head names it; a gap that carries no count says "findings". */
function findingsName(count: number | null): string {
  if (count === null) return "findings";
  const words = COUNT_WORDS[count] ?? String(count);
  return count === 1 ? `${words} finding` : `${words} findings`;
}

function triageHead(feature: string, count: number | null): FrontierHead {
  return {
    verb: "Triage",
    name: findingsName(count),
    nameIsSlug: false,
    reason: "From the last run",
    detail: null,
    glyph: "findings",
    to: featureHref(feature, "findings"),
  };
}

/** The settled plan, if any, that covers a revised section and so reads `upstream_changed`. */
function flaggedPlanDetail(
  trace: TraceView,
  feature: string,
  doc: TraceDoc | undefined,
): string | null {
  if (doc?.role !== "section") return null;
  const plan = coveringPlans(trace, doc.path).find(
    (candidate) => candidate.slug !== null && shownSettle(candidate) === "upstream_changed",
  );
  if (!plan?.slug) return null;
  return `Plan ${shortSlug(feature, plan.slug)} implements it, so it is flagged too.`;
}

/** The head for a gap on one document: it moved since it was settled, or it waits to be settled. */
function docHead(trace: TraceView, feature: string, gap: Gap): FrontierHead {
  const doc = docOfPath(trace, gap.node.path);
  const title = doc?.title ?? fileName(gap.node.path);
  const to = doc ? summaryHref(feature, doc) : featureSourceHref(feature, gap.node.path);
  const titled = { name: title, nameIsSlug: false, detail: null, to };
  const review = { verb: "Review", glyph: "attention" } as const;
  if (gap.kind === "upstream_changed") {
    const named = { ...titled, ...planName(feature, doc, title) };
    return {
      ...review,
      ...named,
      reason: "Its spec changed since you settled it",
    };
  }
  if (gap.kind === "stub_changed") {
    return {
      ...review,
      ...titled,
      reason: "Its stub changed since you settled it",
    };
  }
  if (gap.kind === "revised") {
    const detail = flaggedPlanDetail(trace, feature, doc);
    return {
      ...review,
      ...titled,
      detail,
      reason: "Changed since you settled it",
    };
  }
  return { verb: "Read", glyph: "draft", ...titled, reason: "Not settled yet" };
}

/** The trace's first frontier gap as the spine head says it; null when nothing needs attention. */
export function frontierHead(trace: TraceView, feature: string): FrontierHead | null {
  const gap = trace.frontier[0];
  if (!gap) return null;
  if (gap.kind === "revisit") return revisitHead(trace, feature, gap);
  if (gap.kind === "triage") return triageHead(feature, gap.count);
  if (gap.kind === "uncovered") {
    const heading = headingsOf(trace, gap.node)[0];
    const name = heading?.title ?? gap.node.anchor ?? fileName(gap.node.path);
    return {
      verb: "Cover",
      name,
      nameIsSlug: false,
      reason: "No plan covers this heading",
      detail: null,
      glyph: "not-written",
      to: coverageHref(feature, gap.node.path, gap.node.anchor),
    };
  }
  return docHead(trace, feature, gap);
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
  return inside.length > 0 ? inside.join("/") : fileName(path);
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
      return state !== null && isMoved(state);
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
