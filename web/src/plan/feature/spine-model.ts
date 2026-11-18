import type { ChecklistView, MarkView, StepView } from "@/plan/api/schema";
import type { DocRole, SettleState, TraceView } from "@/plan/api/schema-trace";
import { featurePage } from "@/plan/feature/page-offer";
import { STEP } from "@/plan/feature/steps";
import { featureOfDocPath } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";
import { instrumentChild, type Instrument } from "@/plan/feature/spine-instruments";
import { decode, docRel } from "@/plan/files/address";
import { settleWords } from "@/plan/trace/doc-model";
import { docOfPath, isMoved, shortSlug, shownSettle } from "@/plan/trace/model";

export type GroupId = "vision" | "product" | "metaplan" | "plans";

/** How a document's name is set: section names are Markdown phrasing, plan names are slugs. */
export type NameStyle = "markdown" | "mono" | "plain";

/**
 * The one glyph that leads a row. `attention` marks a changed settled document or a done group
 * with unsettled children; `retired` is a plan out of the workflow.
 */
export type GlyphKind =
  | "running"
  | "attention"
  | "settled"
  | "done"
  | "not-written"
  | "draft"
  | "retired";

export interface RowGlyph {
  kind: GlyphKind;
  /** The glyph's word, for its tooltip and `.sr-only` text; null where the row says it itself. */
  word: string | null;
}

/**
 * The glyph of a row, by precedence: running, attention (amber), settled, a done plan, not written,
 * else draft. A plan out of the workflow (`retired`) comes first. A draft names itself only once the
 * trace says so, so a row never claims "Draft" before the trace loads.
 */
export function rowGlyph(input: {
  written: boolean;
  settle: SettleState | null;
  /** The kind of document the row stands for; its words depend on it (`settleWords`). */
  role: DocRole;
  /** A plan row's state from the checklist; null for any other row. */
  planState: string | null;
}): RowGlyph {
  const { written, settle, role, planState } = input;
  const word = settle === null ? null : settleWords(settle, role).chip;
  if (planState === "retired") return { kind: "retired", word: "Retired" };
  if (planState === "running") return { kind: "running", word: "Running" };
  if (settle !== null && isMoved(settle)) return { kind: "attention", word };
  if (settle === "settled") return { kind: "settled", word };
  if (planState === "done") return { kind: "done", word: "Done" };
  if (!written) return { kind: "not-written", word: null };
  return { kind: "draft", word: settle === "draft" ? word : null };
}

/** A plan's pressure-test record: the date of the last test, null while it was never tested. */
export interface PlanPressure {
  date: string | null;
}

export interface SpineChild {
  key: string;
  /** Null for a document that is not written: it is listed, not linked. */
  to: string | null;
  name: string;
  style: NameStyle;
  /** A section's number, set in mono before its name. */
  number: string | null;
  /** The full name when `name` is shortened (a plan's slug without its feature prefix). */
  title: string | null;
  /** The word beside the name where the glyph carries none: "not written". */
  note: string | null;
  /** A plan's pressure record, read aloud only; null for every other child and a retired plan. */
  pressure: PlanPressure | null;
  /** The row's leading glyph; null for the overview row, which stands for the group itself, and
   *  for an instrument row, which leads with the instrument's own mark. */
  glyph: RowGlyph | null;
  /** The ledger the row reads (Coverage, Findings); null for a document row. */
  instrument: Instrument | null;
}

export interface SpineGroup {
  id: GroupId;
  label: string;
  to: string;
  mark: MarkView;
  /** The word that goes with the mark; the server's count, when it sent one, goes beside it. */
  word: string;
  count: string | null;
  /** What the count counts ("untested", "done"), lowercased from the server's label; null when the
   *  label is only the step's name. */
  unit: string | null;
  children: SpineChild[];
  /** The vision/metaplan document glyph, or attention when a done group's children need settling. */
  glyph: RowGlyph | null;
  /** Whether the first child is the group's own overview page, which shares the group's target. */
  overview: boolean;
}

/** The server's step number behind each group; the labels are the groups', not the server's. */
const GROUPS: { id: GroupId; label: string; step: number; page: string }[] = [
  { id: "vision", label: "Vision", step: STEP.vision, page: "vision" },
  { id: "product", label: "Product spec", step: STEP.product, page: "product" },
  { id: "metaplan", label: "Metaplan", step: STEP.metaplan, page: "metaplan" },
  { id: "plans", label: "Plans", step: STEP.plans, page: "metaplan" },
];

/** The server labels a count's unit ("Untested", "Done"); a label equal to the name names no unit. */
function unitOf(server: StepView | undefined, count: string | null): string | null {
  return server && count !== null && server.label !== server.name
    ? server.label.toLowerCase()
    : null;
}

/** The page segment each group lives under, for finding the group of a pathname. */
const GROUP_OF_PAGE: Record<string, GroupId> = {
  vision: "vision",
  product: "product",
  metaplan: "metaplan",
  engineering: "plans",
  plans: "plans",
  // The instrument rows close the group they read: Coverage the Product spec, Findings the Plans.
  coverage: "product",
  findings: "plans",
};

/** A plan file name, lifecycle prefix included (`PLAN-x.md`, `IN_PROGRESS-PLAN-x.md`). */
const PLAN_FILE = /^(?:[A-Z_]+-)?PLAN-/;

/**
 * A run review file (`REVIEW-PLAN-x.md`, `REVIEW-DONE-PLAN-x.md`), which belongs to the Plans group
 * with the plan it reviews. `PLAN_FILE` alone misses `REVIEW-DONE-PLAN-`: its prefix has no dash.
 */
const REVIEW_FILE = /^REVIEW-/;

/**
 * The group a feature document belongs to: its vision, a section, a plan, the metaplan
 * (`doc/plans/<f>/index.md`, the only one) or a run review. Other files under `doc/plans/<f>/`
 * (briefs, stray notes) belong to no group, as the server classifies them as Other.
 */
function groupOfDocPath(repoPath: string): GroupId | null {
  const [root, , ...rest] = docRel(repoPath).split("/");
  const file = rest.at(-1) ?? "";
  if (root === "product") return rest.length === 1 && file === "index.md" ? "vision" : "product";
  if (root !== "plans") return null;
  if (REVIEW_FILE.test(file) || PLAN_FILE.test(file)) return "plans";
  return rest.length === 1 && file === "index.md" ? "metaplan" : null;
}

/**
 * The group of `/f/<feature>/source/<rel>`: the one of the document it shows, none when the
 * document belongs to another feature than the route's.
 */
function groupOfSource(pathname: string): GroupId | null {
  const repoPath = sourceRepoPath(pathname);
  const [, feature] = pathname.split("/").filter(Boolean);
  if (repoPath === null || featureOfDocPath(repoPath) !== feature) return null;
  return groupOfDocPath(repoPath);
}

export function groupOfPathname(pathname: string): GroupId | null {
  const page = featurePage(pathname)?.page;
  if (page === undefined) return null;
  return page === "source" ? groupOfSource(pathname) : (GROUP_OF_PAGE[page] ?? null);
}

/** The settle state a row shows: none for a listed-only document or done work (`shownSettle`). */
function settleOf(trace: TraceView | null, repoPath: string): SettleState | null {
  return trace ? shownSettle(docOfPath(trace, repoPath)) : null;
}

function overviewChild(feature: string): SpineChild {
  return {
    key: "overview",
    to: featureHref(feature, "product"),
    name: "Overview",
    style: "plain",
    number: null,
    title: null,
    note: null,
    pressure: null,
    glyph: null,
    instrument: null,
  };
}

function sectionChildren(checklist: ChecklistView, trace: TraceView | null): SpineChild[] {
  return checklist.sections.map((section) => ({
    key: section.number,
    to: section.written
      ? featureHref(checklist.feature, `product/section_${section.number}`)
      : null,
    name: section.name,
    style: "markdown",
    number: section.number,
    title: null,
    note: section.written ? null : "not written",
    pressure: null,
    instrument: null,
    glyph: rowGlyph({
      written: section.written,
      role: "section",
      planState: null,
      settle: section.written
        ? settleOf(trace, `doc/product/${checklist.feature}/section_${section.number}.md`)
        : null,
    }),
  }));
}

function planChildren(checklist: ChecklistView, trace: TraceView | null): SpineChild[] {
  return checklist.plan_rows.map((plan) => ({
    key: plan.slug,
    to: featureHref(checklist.feature, `engineering/${encodeURIComponent(plan.slug)}`),
    name: shortSlug(checklist.feature, plan.slug),
    style: "mono",
    number: null,
    title: plan.slug,
    note: null,
    pressure: plan.state === "retired" ? null : { date: plan.pressure_date },
    instrument: null,
    glyph: rowGlyph({
      written: true,
      role: "plan",
      planState: plan.state,
      settle: settleOf(trace, plan.path),
    }),
  }));
}

const MARK_WORDS: Record<Exclude<MarkView, "gap">, string> = {
  done: "done",
  active: "in progress",
  optional: "optional",
};

/** The word for a group's mark; a group with nothing in it yet reads "not started". */
function wordOf(id: GroupId, mark: MarkView, checklist: ChecklistView): string {
  if (id === "product" && checklist.sections.length === 0) return "not started";
  if (mark !== "gap") return MARK_WORDS[mark];
  if (id === "vision" || id === "metaplan") return "not written";
  if (id === "plans") return checklist.plan_rows.length === 0 ? "not started" : "has gaps";
  return id === "product" ? "has gaps" : "not started";
}

/** The repository path a source route shows, or null for a route that is not one or is unreadable. */
function sourceRepoPath(pathname: string): string | null {
  const [root, , page, ...rel] = pathname.split("/").filter(Boolean);
  if (root !== "f" || page !== "source") return null;
  const parts = rel.map(decode);
  return parts.includes(null) ? null : `doc/${parts.join("/")}`;
}

/** The key (see `SpineChild.key`) of the child that lists a document of `group`, if any does. */
function childKeyOf(checklist: ChecklistView, group: GroupId, repoPath: string): string | null {
  const same = (path: string) => docRel(path) === docRel(repoPath);
  if (group === "plans") return checklist.plan_rows.find((plan) => same(plan.path))?.slug ?? null;
  if (group === "metaplan")
    return checklist.metaplans.find((item) => same(item.path))?.path ?? null;
  if (group !== "product") return null;
  const parts = docRel(repoPath).split("/");
  return parts.length === 3 ? (/^section_(.+)\.md$/.exec(parts[2]!)?.[1] ?? null) : null;
}

/**
 * The summary route of the document a source route shows (`/f/<f>/source/<rel>`): the `to` of the
 * spine child that lists it, the group's own target for a vision or a lone metaplan (which has no
 * child), and null when no row stands for the document.
 */
export function summaryPathnameOfSource(checklist: ChecklistView, pathname: string): string | null {
  const repoPath = sourceRepoPath(pathname);
  if (repoPath === null || featureOfDocPath(repoPath) !== checklist.feature) return null;
  const id = groupOfDocPath(repoPath);
  const group = spineGroups(checklist).find((candidate) => candidate.id === id);
  if (!group) return null;
  if (group.id === "vision") return group.to;
  const key = childKeyOf(checklist, group.id, repoPath);
  if (key === null) return null;
  if (group.id === "metaplan") return group.to;
  // The overview child (first, where a group has one) shares the group's target and lists no document.
  const listed = group.overview ? group.children.slice(1) : group.children;
  return listed.find((candidate) => candidate.key === key)?.to ?? null;
}

/**
 * The pathname whose spine row is the current one. A source route stands for its summary row
 * (`summaryPathnameOfSource`); every other route stands for itself.
 */
export function currentPathname(checklist: ChecklistView, pathname: string): string {
  return summaryPathnameOfSource(checklist, pathname) ?? pathname;
}

function childrenOf(id: GroupId, checklist: ChecklistView, trace: TraceView | null): SpineChild[] {
  // Each group's instrument closes it: the ledger read over the documents above it.
  if (id === "product") {
    return [
      overviewChild(checklist.feature),
      ...sectionChildren(checklist, trace),
      instrumentChild("coverage", checklist.feature, trace),
    ];
  }
  if (id === "plans") {
    return [
      ...planChildren(checklist, trace),
      instrumentChild("findings", checklist.feature, trace),
    ];
  }
  return [];
}

/** Document glyphs for vision/metaplan; attention on done groups with unsettled children. */
function groupGlyph(
  id: GroupId,
  mark: MarkView,
  trace: TraceView | null,
  children: SpineChild[],
): RowGlyph | null {
  if (id === "product" || id === "plans") {
    // Without a trace every child glyph is a placeholder, so nothing is known to be unsettled yet.
    if (trace === null) return null;
    const unsettled = children.some(
      ({ glyph }) => glyph !== null && !["settled", "done", "retired"].includes(glyph.kind),
    );
    return mark === "done" && unsettled ? { kind: "attention", word: "Not all settled" } : null;
  }
  const doc = trace?.documents.find((candidate) => candidate.role === id);
  return rowGlyph({ written: mark !== "gap", role: id, planState: null, settle: shownSettle(doc) });
}

/** The workflow groups; even an empty feature shows them all. `trace` adds document settle states. */
export function spineGroups(
  checklist: ChecklistView,
  trace: TraceView | null = null,
): SpineGroup[] {
  return GROUPS.map(({ id, label, step, page }) => {
    const server: StepView | undefined = checklist.steps.find(
      (candidate) => candidate.number === step,
    );
    const mark = server?.mark ?? "gap";
    const count = id === "vision" || id === "metaplan" ? null : (server?.count ?? null);
    const children = childrenOf(id, checklist, trace);
    return {
      id,
      label,
      to: featureHref(checklist.feature, page),
      mark,
      word: wordOf(id, mark, checklist),
      count,
      unit: unitOf(server, count),
      children,
      glyph: groupGlyph(id, mark, trace, children),
      overview: id === "product",
    };
  });
}
