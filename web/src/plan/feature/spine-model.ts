import type { LucideIcon } from "lucide-react";

import type { ChecklistView, MarkView, StepView } from "@/plan/api/schema";
import { featurePage } from "@/plan/feature/page-offer";
import { featureHref } from "@/plan/feature/use-feature";
import { PLAN_STATE_GLYPHS, type PlanState } from "@/plan/ui/glyphs";

export type GroupId = "vision" | "product" | "metaplan" | "plans" | "pressure" | "run";

/** How a document's name is set: section names are Markdown phrasing, plan names are slugs. */
export type NameStyle = "markdown" | "mono" | "plain";

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
  icon: LucideIcon | null;
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
  /** Whether the first child is the group's own overview page, which shares the group's target. */
  overview: boolean;
}

/** The server's step number behind each group; the labels are the groups', not the server's. */
const GROUPS: { id: GroupId; label: string; step: number; page: string }[] = [
  { id: "vision", label: "Vision", step: 1, page: "vision" },
  { id: "product", label: "Product spec", step: 2, page: "product" },
  { id: "metaplan", label: "Metaplan", step: 3, page: "metaplan" },
  { id: "plans", label: "Plans", step: 4, page: "engineering" },
  { id: "pressure", label: "Pressure test", step: 5, page: "pressure" },
  { id: "run", label: "Run", step: 6, page: "run" },
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

export function groupOfPathname(pathname: string): GroupId | null {
  const page = featurePage(pathname)?.page;
  return page === undefined ? null : (GROUP_OF_PAGE[page] ?? null);
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
  };
}

function sectionChildren(checklist: ChecklistView): SpineChild[] {
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
  }));
}

function planChildren(checklist: ChecklistView): SpineChild[] {
  return checklist.plan_rows.map((plan) => ({
    key: plan.slug,
    to: featureHref(checklist.feature, `engineering/${encodeURIComponent(plan.slug)}`),
    name: plan.slug,
    style: "mono",
    number: null,
    note: plan.state,
    icon: plan.state in PLAN_STATE_GLYPHS ? PLAN_STATE_GLYPHS[plan.state as PlanState] : null,
  }));
}

/** A metaplan file is named in the route by its file name. */
export function metaplanFile(path: string): string {
  return path.split("/").at(-1) ?? path;
}

function metaplanChildren(checklist: ChecklistView): SpineChild[] {
  if (checklist.metaplans.length < 2) return [];
  return checklist.metaplans.map((metaplan) => ({
    key: metaplan.path,
    to: featureHref(
      checklist.feature,
      `metaplan/${encodeURIComponent(metaplanFile(metaplan.path))}`,
    ),
    name: metaplan.title,
    style: "plain",
    number: null,
    note: null,
    icon: null,
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

function childrenOf(id: GroupId, checklist: ChecklistView, page: string): SpineChild[] {
  if (id === "product") {
    return [overviewChild(checklist.feature, page), ...sectionChildren(checklist)];
  }
  if (id === "plans") return [overviewChild(checklist.feature, page), ...planChildren(checklist)];
  return id === "metaplan" ? metaplanChildren(checklist) : [];
}

/** The spine's groups in workflow order. A feature with nothing yet hides no group. */
export function spineGroups(checklist: ChecklistView): SpineGroup[] {
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
      children: childrenOf(id, checklist, page),
      overview: id === "product" || id === "plans",
    };
  });
}
