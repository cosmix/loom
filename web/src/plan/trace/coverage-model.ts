import type {
  Finding,
  ProgressKind,
  SettleState,
  TraceDoc,
  TraceHeading,
  TraceView,
} from "@/plan/api/schema-trace";
import { fileName } from "@/plan/concept/passages";
import { featureSourceHref } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";
import { docOfPath, shortSlug, shownSettle, summaryHref } from "@/plan/trace/model";

// The coverage page as data: the spec grouped by section, one row per heading with the plans and
// stages that cover it, the four counts that double as filters. Pure reads of the trace;
// `coverage-page.tsx` and `coverage-rows.tsx` draw it.

/** What a heading row reads: built work, planned work, a plan whose spec moved, or nothing. */
export type RowState = "built" | "planned" | "changed" | "uncovered";

export const STATE_WORDS: Record<RowState, string> = {
  built: "Built",
  planned: "Planned",
  changed: "Changed since covered",
  uncovered: "Not covered",
};

export type Filter = "all" | "uncovered" | "changed" | "open";

/** The four tiles, in order; each is also the filter that shows its rows. */
export const TILES: readonly { id: Filter; label: string }[] = [
  { id: "all", label: "All headings" },
  { id: "uncovered", label: "Not covered" },
  { id: "changed", label: "Changed since covered" },
  { id: "open", label: "With open findings" },
];

const PLAN_WORDS: Record<ProgressKind, string> = {
  stub: "Not written",
  written: "Written",
  planned: "Written",
  built: "Built",
  running: "Running",
  done: "Done",
};

/** One way a heading is covered: a metaplan entry, the plan it leads to, and the stage if any. */
export interface Chain {
  entry: string | null;
  entryHref: string | null;
  /** The plan's short slug; the entry's slug while its plan is unwritten. */
  plan: string;
  planPath: string | null;
  /** Where the plan's summary page is; null for a plan that is only a metaplan stub. */
  planHref: string | null;
  /** The plan's state in words: "Not written", "Written", "Running", "Done". */
  planState: string;
  /** The plan's work is finished. */
  done: boolean;
  stage: string | null;
  stageHref: string | null;
  /** The plan is settled against a spec that has since changed. */
  changed: boolean;
}

export interface CoverageRow {
  /** `<path>#<anchor>`, unique across the page. */
  key: string;
  anchor: string;
  title: string;
  chains: Chain[];
  /** The findings about this heading that still wait for a decision. */
  open: Pick<Finding, "id" | "title">[];
  state: RowState;
}

export interface CoverageSection {
  /** The file's name without `.md` (`section_04`): the address of the section on the page. */
  id: string;
  number: string | null;
  title: string;
  path: string;
  doc: TraceDoc;
  /** Null for a section the reader is not asked to settle. */
  settle: SettleState | null;
  written: boolean;
  covered: number;
  total: number;
  rows: CoverageRow[];
}

export type Tiles = Record<Filter, number>;

export interface CoverageModel {
  tiles: Tiles;
  sections: CoverageSection[];
}

/** Done work, or work built and waiting for its run to close. */
function isDone(doc: TraceDoc | undefined): boolean {
  return doc?.progress === "done" || doc?.progress === "built";
}

function chainOf(
  trace: TraceView,
  feature: string,
  cover: TraceHeading["covered_by"][number],
): Chain {
  const entry =
    cover.kind === "entry"
      ? trace.entries.find((candidate) => candidate.anchor === cover.anchor)
      : trace.entries.find((candidate) => candidate.plan === cover.path);
  const planPath = cover.kind === "entry" ? (entry?.plan ?? null) : cover.path;
  const doc = planPath === null ? undefined : docOfPath(trace, planPath);
  const stage = cover.kind === "stage" ? (cover.stage ?? cover.anchor) : null;
  const slug = doc?.slug ?? cover.slug ?? entry?.slug ?? fileName(planPath ?? cover.path);
  return {
    entry: entry?.title ?? null,
    entryHref: entry
      ? `${featureHref(feature, "metaplan")}#${encodeURIComponent(entry.anchor)}`
      : null,
    plan: shortSlug(feature, slug),
    planPath,
    planHref: doc && doc.progress !== "stub" ? summaryHref(feature, doc) : null,
    planState: doc ? PLAN_WORDS[doc.progress] : PLAN_WORDS.stub,
    done: isDone(doc),
    stage,
    stageHref:
      stage !== null && planPath !== null
        ? featureSourceHref(feature, planPath, { anchor: `stage:${stage}` })
        : null,
    changed: shownSettle(doc) === "upstream_changed",
  };
}

/**
 * The chains covering a heading, one per entry/plan/stage path. An entry and a plan that name the
 * same plan are one chain, and a plan-wide cover gives way to the stages that cover on their own.
 */
function chainsOf(trace: TraceView, feature: string, heading: TraceHeading): Chain[] {
  const chains = new Map<string, Chain>();
  for (const cover of heading.covered_by) {
    const chain = chainOf(trace, feature, cover);
    const owner = chain.planPath ?? `entry:${cover.anchor ?? cover.path}`;
    chains.set(`${owner}|${chain.stage ?? ""}`, chain);
  }
  const staged = new Set(
    [...chains.values()].filter((c) => c.stage !== null).map((c) => c.planPath),
  );
  return [...chains.values()].filter(
    (chain) => chain.stage !== null || !staged.has(chain.planPath),
  );
}

function stateOf(chains: Chain[]): RowState {
  if (chains.length === 0) return "uncovered";
  if (chains.some((chain) => chain.changed)) return "changed";
  return chains.every((chain) => chain.done) ? "built" : "planned";
}

function rowOf(trace: TraceView, feature: string, heading: TraceHeading): CoverageRow {
  const chains = chainsOf(trace, feature, heading);
  const open = heading.findings.flatMap((id) => {
    const found = trace.findings.find(({ finding }) => finding.id === id)?.finding;
    return found?.status.kind === "open" ? [{ id: found.id, title: found.title }] : [];
  });
  return {
    key: `${heading.path}#${heading.anchor}`,
    anchor: heading.anchor,
    title: heading.title,
    chains,
    open,
    state: stateOf(chains),
  };
}

function sectionOf(trace: TraceView, feature: string, doc: TraceDoc): CoverageSection {
  const written = doc.progress !== "stub";
  const rows = written
    ? trace.headings.filter((h) => h.path === doc.path).map((h) => rowOf(trace, feature, h))
    : [];
  return {
    id: fileName(doc.path).replace(/\.md$/, ""),
    number: doc.number === null ? null : String(doc.number).padStart(2, "0"),
    title: doc.title,
    path: doc.path,
    doc,
    settle: shownSettle(doc),
    written,
    covered: rows.filter((row) => row.state !== "uncovered").length,
    total: rows.length,
    rows,
  };
}

/** The coverage page of a feature's trace: its sections in order, and the tile counts. */
export function buildCoverage(trace: TraceView, feature: string): CoverageModel {
  const byNumber = (a: TraceDoc, b: TraceDoc) =>
    (a.number ?? Number.MAX_SAFE_INTEGER) - (b.number ?? Number.MAX_SAFE_INTEGER);
  const sections = trace.documents
    .filter((doc) => doc.role === "section")
    .sort(byNumber)
    .map((doc) => sectionOf(trace, feature, doc));
  const rows = sections.flatMap((section) => section.rows);
  const count = (filter: Filter) => rows.filter((row) => matches(row, filter)).length;
  return {
    tiles: {
      all: rows.length,
      uncovered: count("uncovered"),
      changed: count("changed"),
      open: count("open"),
    },
    sections,
  };
}

function matches(row: CoverageRow, filter: Filter): boolean {
  if (filter === "uncovered") return row.state === "uncovered";
  if (filter === "changed") return row.state === "changed";
  if (filter === "open") return row.open.length > 0;
  return true;
}

/**
 * The sections a filter shows, each holding only the rows it picks. A filter drops the sections
 * left with no row; `all` keeps every section, written or not. A section keeps its own
 * covered and total counts whatever the filter.
 */
export function filterRows(model: CoverageModel, filter: Filter): CoverageSection[] {
  if (filter === "all") return model.sections;
  return model.sections
    .map((section) => ({ ...section, rows: section.rows.filter((row) => matches(row, filter)) }))
    .filter((section) => section.rows.length > 0);
}

/** The element id of a heading row; heading anchors repeat across sections, section ids do not. */
export function rowElementId(section: CoverageSection, row: CoverageRow): string {
  return `${section.id}--${row.anchor}`;
}

/**
 * What a URL fragment names: a heading as `<section file name>:<anchor>` (`section_04:two`, what
 * `coverageHref` writes), a section by its file name (`section_04`) or a heading by its bare
 * anchor (the first section's, when several share it). A heading's row opens when the fragment
 * names it.
 */
export function findAnchor(
  model: CoverageModel,
  anchor: string,
): { elementId: string; rowKey: string | null } | null {
  const section = model.sections.find((candidate) => candidate.id === anchor);
  if (section) return { elementId: section.id, rowKey: null };
  const colon = anchor.indexOf(":");
  if (colon > 0) {
    const owner = model.sections.find((candidate) => candidate.id === anchor.slice(0, colon));
    const row = owner?.rows.find((candidate) => candidate.anchor === anchor.slice(colon + 1));
    if (owner && row) return { elementId: rowElementId(owner, row), rowKey: row.key };
  }
  for (const owner of model.sections) {
    const row = owner.rows.find((candidate) => candidate.anchor === anchor);
    if (row) return { elementId: rowElementId(owner, row), rowKey: row.key };
  }
  return null;
}
