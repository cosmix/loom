import type { ChecklistView, MarkView, StepView } from "@/plan/api/schema";
import type { SettleState, TraceView } from "@/plan/api/schema-trace";
import { featurePage } from "@/plan/feature/page-offer";
import { STEP } from "@/plan/feature/steps";
import { featureOfDocPath } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";
import { decode, docRel } from "@/plan/files/address";
import { docOfPath } from "@/plan/trace/model";
import { PLAN_STATE_GLYPHS, type Glyph, type PlanState } from "@/plan/ui/glyphs";

export type GroupId = "vision" | "product" | "metaplan" | "plans" | "run";

/** How a document's name is set: section names are Markdown phrasing, plan names are slugs. */
export type NameStyle = "markdown" | "mono" | "plain";

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
  /** The word beside the name: a plan's state, or "not written". */
  note: string | null;
  icon: Glyph | null;
  /** A plan's pressure record; null for every other child and for a retired plan. */
  pressure: PlanPressure | null;
  /** The document's settle state from the trace; null for a document with no body to settle. */
  settle: SettleState | null;
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
  /** The settle state of the vision and metaplan rows; null for every other group. */
  settle: SettleState | null;
  /** Whether the first child is the group's own overview page, which shares the group's target. */
  overview: boolean;
}

/** The server's step number behind each group; the labels are the groups', not the server's. */
const GROUPS: { id: GroupId; label: string; step: number; page: string }[] = [
  { id: "vision", label: "Vision", step: STEP.vision, page: "vision" },
  { id: "product", label: "Product spec", step: STEP.product, page: "product" },
  { id: "metaplan", label: "Metaplan", step: STEP.metaplan, page: "metaplan" },
  { id: "plans", label: "Plans", step: STEP.plans, page: "engineering" },
  { id: "run", label: "Run", step: STEP.run, page: "run" },
];

/** The server labels a count's unit ("Untested", "Done"); a label equal to the name names no unit. */
function unitOf(server: StepView | undefined, count: string | null): string | null {
  return server && count !== null && server.label !== server.name
    ? server.label.toLowerCase()
    : null;
}

/** The page segment each group lives under, for finding the group of a pathname. */
const GROUP_OF_PAGE: Record<string, GroupId> = Object.fromEntries(
  GROUPS.map((group) => [group.page, group.id]),
);

/** A plan file name, lifecycle prefix included (`PLAN-x.md`, `IN_PROGRESS-PLAN-x.md`). */
const PLAN_FILE = /^(?:[A-Z_]+-)?PLAN-/;

/**
 * A run review file (`REVIEW-PLAN-x.md`, `REVIEW-DONE-PLAN-x.md`), which the Run group shows.
 * Tested before `PLAN_FILE`, whose optional prefix also matches `REVIEW-`.
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
  if (REVIEW_FILE.test(file)) return "run";
  if (PLAN_FILE.test(file)) return "plans";
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

/** The settle state of a written document; a listed-only document has nothing to settle. */
function settleOf(trace: TraceView | null, repoPath: string): SettleState | null {
  const doc = trace ? docOfPath(trace, repoPath) : undefined;
  return doc && doc.progress !== "stub" ? doc.settle : null;
}

function overviewChild(feature: string, page: string): SpineChild {
  return {
    key: "overview",
    to: featureHref(feature, page),
    name: "Overview",
    style: "plain",
    number: null,
    note: null,
    icon: null,
    pressure: null,
    settle: null,
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
    note: section.written ? null : "not written",
    icon: null,
    pressure: null,
    settle: section.written
      ? settleOf(trace, `doc/product/${checklist.feature}/section_${section.number}.md`)
      : null,
  }));
}

function planChildren(checklist: ChecklistView, trace: TraceView | null): SpineChild[] {
  return checklist.plan_rows.map((plan) => ({
    key: plan.slug,
    to: featureHref(checklist.feature, `engineering/${encodeURIComponent(plan.slug)}`),
    name: plan.slug,
    style: "mono",
    number: null,
    note: plan.state,
    icon: Object.hasOwn(PLAN_STATE_GLYPHS, plan.state)
      ? PLAN_STATE_GLYPHS[plan.state as PlanState]
      : null,
    pressure: plan.state === "retired" ? null : { date: plan.pressure_date },
    settle: settleOf(trace, plan.path),
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

function childrenOf(
  id: GroupId,
  checklist: ChecklistView,
  page: string,
  trace: TraceView | null,
): SpineChild[] {
  if (id === "product") {
    return [overviewChild(checklist.feature, page), ...sectionChildren(checklist, trace)];
  }
  if (id === "plans") {
    return [overviewChild(checklist.feature, page), ...planChildren(checklist, trace)];
  }
  return [];
}

/** The settle state of the vision or metaplan row: the trace's one document of that role. */
function groupSettle(id: GroupId, trace: TraceView | null): SettleState | null {
  const role = id === "vision" || id === "metaplan" ? id : null;
  return role ? (trace?.documents.find((doc) => doc.role === role)?.settle ?? null) : null;
}

/**
 * The spine's groups in workflow order. A feature with nothing yet hides no group. `trace` adds
 * each document's settle state; without it (not loaded yet) no row shows one.
 */
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
    return {
      id,
      label,
      to: featureHref(checklist.feature, page),
      mark,
      word: wordOf(id, mark, checklist),
      count,
      unit: unitOf(server, count),
      children: childrenOf(id, checklist, page, trace),
      settle: groupSettle(id, trace),
      overview: id === "product" || id === "plans",
    };
  });
}
