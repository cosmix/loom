import { traceViewSchema, type TraceView } from "@/plan/api/schema-trace";

// The trace behind the Help scene's spine, so its Coverage and Findings wells show readings
// without a request. It matches the scene's checklist (`getting-around-scene.tsx`): five written
// sections and a stub, and two plans. Two headings are covered by no plan (sections 03 and 04),
// two findings are open and two decided, three of them from one run review.

const FEATURE = "atlas";
const PLAN_LOOP = `doc/plans/${FEATURE}/PLAN-atlas-01-loop.md`;
const PLAN_SYNC = `doc/plans/${FEATURE}/PLAN-atlas-02-sync.md`;
const RUN_SOURCE = "REVIEW-PLAN-atlas-01-loop.md#open-findings (build, 1a2b3c4d5e6f)";

export const SAMPLE_SECTIONS = [
  { number: "01", name: "Project map", written: true },
  { number: "02", name: "Search", written: true },
  { number: "03", name: "Archive", written: true },
  { number: "04", name: "Sharing", written: true },
  { number: "05", name: "Settings", written: true },
  { number: "06", name: "Exports", written: false },
] as const;

export const SAMPLE_PLANS = [
  { slug: "atlas-01-loop", path: PLAN_LOOP, title: "Project map", pressure: "2026-10-30" },
  { slug: "atlas-02-sync", path: PLAN_SYNC, title: "Search", pressure: null },
] as const;

const SETTLED = { body: "aaaaaaaaaaaa", stub: null, upstream: null };

function sectionPath(number: string): string {
  return `doc/product/${FEATURE}/section_${number}.md`;
}

function document(path: string, role: "section" | "plan", title: string, unwritten: boolean) {
  const number = /section_(\d+)\.md$/.exec(path)?.[1];
  return {
    path,
    role,
    title,
    progress: unwritten ? "stub" : "written",
    settle: "settled",
    settleable: !unwritten,
    stub: null,
    number: number === undefined ? null : Number(number),
    slug: role === "plan" ? (/PLAN-(.+)\.md$/.exec(path)?.[1] ?? null) : null,
    bundle: [],
    my_calls: 0,
    open_questions: 0,
    current: SETTLED,
  };
}

/** A heading of a section, covered by the plan at `plan` or by none. */
function heading(number: string, anchor: string, plan: string | null) {
  return {
    path: sectionPath(number),
    anchor,
    title: anchor.replace(/^./, (first) => first.toUpperCase()),
    covered_by:
      plan === null ? [] : [{ kind: "plan", path: plan, anchor: null, slug: null, stage: null }],
    findings: [],
  };
}

function finding(id: string, title: string, kind: string, source: string | null) {
  return {
    finding: {
      id,
      title,
      about: [],
      source,
      status: { kind, detail: null },
      text: title,
    },
    about: [],
  };
}

export const SAMPLE_TRACE: TraceView = traceViewSchema.parse({
  documents: [
    ...SAMPLE_SECTIONS.map(({ number, name, written }) =>
      document(sectionPath(number), "section", name, !written),
    ),
    ...SAMPLE_PLANS.map(({ path, title }) => document(path, "plan", title, false)),
  ],
  headings: [
    heading("01", "overview", PLAN_LOOP),
    heading("01", "navigation", PLAN_LOOP),
    heading("01", "pinning", PLAN_LOOP),
    heading("02", "queries", PLAN_SYNC),
    heading("02", "filters", PLAN_SYNC),
    heading("03", "archive", PLAN_LOOP),
    heading("03", "restore", null),
    heading("03", "retention", PLAN_LOOP),
    heading("04", "links", PLAN_SYNC),
    heading("04", "access", null),
    heading("05", "themes", PLAN_SYNC),
    heading("05", "shortcuts", PLAN_SYNC),
  ],
  entries: [],
  findings: [
    finding("F-1", "Search ignores archived work", "open", RUN_SOURCE),
    finding("F-2", "Pinned maps lose their order", "open", RUN_SOURCE),
    finding("F-3", "Rename the sync flag", "planned", RUN_SOURCE),
    finding("F-4", "Settings copy is long", "dismissed", null),
  ],
  gaps: [],
  frontier: [],
  problems: [],
});
