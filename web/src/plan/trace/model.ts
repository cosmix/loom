import type { Gap, NodeRef, SettleState, TraceDoc, TraceView } from "@/plan/api/schema-trace";
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

function baseName(path: string): string {
  return path.split("/").at(-1) ?? path;
}

/** Where a heading or a section sits on the Coverage page: the section file's stem, then `:anchor`. */
export function coverageHref(feature: string, path: string, anchor?: string | null): string {
  const stem = baseName(path).replace(/\.md$/, "");
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

/** What the spine head says: a sentence, an optional muted line, and the one button after them. */
export interface FrontierHead {
  sentence: string;
  detail: string | null;
  label: string;
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

/** "Revisit <entry> before writing its plan", or "<plan> before running it" once it is written. */
function revisitHead(trace: TraceView, feature: string, gap: Gap): FrontierHead {
  const doc = docOfPath(trace, gap.node.path);
  const entry = entryAt(trace, gap.node);
  const unwritten = doc?.role === "plan" ? doc.progress === "stub" : entry?.plan === null;
  if (unwritten) {
    const anchor = entry?.anchor ?? gap.node.anchor;
    const name = entry?.title ?? doc?.title ?? baseName(gap.node.path);
    return {
      sentence: `Revisit ${name} before writing its plan.`,
      detail: null,
      label: "Open the metaplan",
      to: `${featureHref(feature, "metaplan")}${anchor ? `#${encodeURIComponent(anchor)}` : ""}`,
    };
  }
  // A gap on a metaplan entry names the entry's written plan.
  const plan = doc?.role === "plan" || !entry?.plan ? doc : docOfPath(trace, entry.plan);
  const title = plan?.title ?? entry?.title ?? baseName(gap.node.path);
  return {
    sentence: `Revisit ${title} before running it.`,
    detail: null,
    label: plan?.slug ? `Open ${shortSlug(feature, plan.slug)}` : "Open the metaplan",
    to: plan ? summaryHref(feature, plan) : featureHref(feature, "metaplan"),
  };
}

/** Counts up to twenty read as words ("Three findings"); a larger count stays a number. */
const COUNT_WORDS = (
  "Zero One Two Three Four Five Six Seven Eight Nine Ten Eleven Twelve Thirteen Fourteen " +
  "Fifteen Sixteen Seventeen Eighteen Nineteen Twenty"
).split(" ");

/** The sentence for `count` findings waiting; a gap that carries no count gets a generic one. */
function triageSentence(count: number | null): string {
  if (count === null) return "Findings from the last run are waiting for you.";
  const words = COUNT_WORDS[count] ?? String(count);
  return count === 1
    ? `${words} finding from the last run is waiting for you.`
    : `${words} findings from the last run are waiting for you.`;
}

function triageHead(feature: string, count: number | null): FrontierHead {
  return {
    sentence: triageSentence(count),
    detail: null,
    label: "Triage findings",
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
  const title = doc?.title ?? baseName(gap.node.path);
  const to = doc ? summaryHref(feature, doc) : featureSourceHref(feature, gap.node.path);
  const plan = doc?.slug ? shortSlug(feature, doc.slug) : title;
  if (gap.kind === "upstream_changed") {
    return {
      sentence: `The spec under ${plan} changed since you settled it.`,
      detail: null,
      label: `Review ${plan}`,
      to,
    };
  }
  if (gap.kind === "stub_changed") {
    const sentence = `${title}'s stub changed since you settled it.`;
    return { sentence, detail: null, label: `Review ${title}`, to };
  }
  if (gap.kind === "revised") {
    const sentence = `${title} changed since you settled it.`;
    const detail = flaggedPlanDetail(trace, feature, doc);
    return { sentence, detail, label: `Review ${title}`, to };
  }
  return { sentence: `Read ${title} and settle it.`, detail: null, label: `Read ${title}`, to };
}

/** The trace's first frontier gap as the spine head says it; null when nothing needs attention. */
export function frontierHead(trace: TraceView, feature: string): FrontierHead | null {
  const gap = trace.frontier[0];
  if (!gap) return null;
  if (gap.kind === "revisit") return revisitHead(trace, feature, gap);
  if (gap.kind === "triage") return triageHead(feature, gap.count);
  if (gap.kind === "uncovered") {
    const heading = headingsOf(trace, gap.node)[0];
    const name = heading?.title ?? gap.node.anchor ?? baseName(gap.node.path);
    return {
      sentence: `${name} is not covered by any plan.`,
      detail: null,
      label: "Open coverage",
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
