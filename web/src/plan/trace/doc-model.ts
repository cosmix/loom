import type { DocView } from "@/plan/api/schema-files";
import type {
  CoverRef,
  DocRole,
  SettleState,
  TraceDoc,
  TraceHeading,
  TraceView,
} from "@/plan/api/schema-trace";
import { featureOfDocPath } from "@/plan/feature/source-href";
import { featureHref } from "@/plan/feature/use-feature";

import { docOfPath, shortSlug, summaryHref } from "./model";

// Pure reads of a feature's trace for the document heading, the plan page, an opened passage and
// the settle confirmation. Nothing here fetches or renders.

function baseName(path: string): string {
  return path.split("/").at(-1) ?? path;
}

// ---- Settle words ----------------------------------------------------------------------------

export interface SettleWords {
  /** The state as a chip or a row reads it ("Settled", "Changed since you settled it"). */
  chip: string;
  /** The settle control's label: what the button says, which for a draft is the verb. */
  button: string;
}

/** The words of a settle state; a plan's `upstream_changed` is its spec that moved. */
export function settleWords(state: SettleState, role: DocRole): SettleWords {
  if (state === "draft") return { chip: "Draft", button: "Mark settled" };
  if (state === "settled") return { chip: "Settled", button: "Settled" };
  if (state === "upstream_changed") {
    return role === "plan"
      ? {
          chip: "Spec changed since you settled it",
          button: "Spec changed since you settled this plan",
        }
      : {
          chip: "Upstream changed since you settled it",
          button: "Upstream changed since you settled it",
        };
  }
  return { chip: "Changed since you settled it", button: "Changed since you settled it" };
}

/** What "See what changed" compares: the settled commit of `settled`, against `compare` now. */
export interface ChangeTarget {
  settled: string;
  compare: string;
}

/**
 * The comparison behind "See what changed". A revised document differs from its settled version;
 * a changed stub is the vision or metaplan file that holds it; a plan whose spec moved compares
 * the first section it covers that is itself revised. Null when nothing names a file to compare.
 */
export function changeTarget(trace: TraceView, doc: TraceDoc): ChangeTarget | null {
  if (doc.settle === "revised") return { settled: doc.path, compare: doc.path };
  if (doc.settle === "stub_changed") {
    return doc.stub ? { settled: doc.path, compare: doc.stub.path } : null;
  }
  if (doc.settle !== "upstream_changed" || doc.role !== "plan") return null;
  const moved = implementedHeadings(trace, doc.path)
    .map(({ section }) => section)
    .find((section) => section?.settle === "revised");
  return moved ? { settled: moved.path, compare: moved.path } : null;
}

// ---- Coverage --------------------------------------------------------------------------------

export interface CoveringPlan {
  /** The plan's slug without its feature prefix. */
  slug: string;
  /** Its summary page, or its metaplan entry while it is not written. */
  to: string;
  /** The stages that cover the heading; empty when the plan covers it as a whole. */
  stages: string[];
}

export interface HeadingCoverage {
  title: string;
  plans: CoveringPlan[];
  /** The open findings about the heading. */
  open: number;
}

function planLink(trace: TraceView, feature: string, cover: CoverRef): string {
  const plan = cover.kind === "entry" ? undefined : docOfPath(trace, cover.path);
  if (plan) return summaryHref(feature, plan);
  const anchor = cover.anchor ? `#${encodeURIComponent(cover.anchor)}` : "";
  return `${featureHref(feature, "metaplan")}${anchor}`;
}

function openFindingCount(trace: TraceView, heading: TraceHeading): number {
  const ids = new Set(heading.findings);
  return trace.findings.filter(
    ({ finding }) => ids.has(finding.id) && finding.status.kind === "open",
  ).length;
}

/**
 * The plans and stages covering a spec heading, and its open findings. A plan or stage cover wins
 * over the metaplan entry of the same plan; an entry alone names a plan not yet written. Null
 * when the trace has no such heading (a file outside the spec).
 */
export function coverageOfHeading(
  trace: TraceView,
  path: string,
  anchor: string,
): HeadingCoverage | null {
  const heading = trace.headings.find((item) => item.path === path && item.anchor === anchor);
  const feature = featureOfDocPath(path);
  if (!heading || feature === null) return null;
  const plans = new Map<string, CoveringPlan>();
  const ordered = [...heading.covered_by].sort(
    (a, b) => Number(a.kind === "entry") - Number(b.kind === "entry"),
  );
  for (const cover of ordered) {
    const key = cover.slug ?? cover.path;
    const known = plans.get(key);
    if (known) {
      if (cover.kind === "stage" && cover.stage) known.stages.push(cover.stage);
      continue;
    }
    const stages = cover.kind === "stage" && cover.stage ? [cover.stage] : [];
    plans.set(key, {
      slug: shortSlug(feature, cover.slug ?? baseName(cover.path)),
      to: planLink(trace, feature, cover),
      stages,
    });
  }
  return {
    title: heading.title,
    plans: [...plans.values()],
    open: openFindingCount(trace, heading),
  };
}

/** How many of a section's headings anything covers (the server's own rule: a cover of any kind). */
export function sectionCoverage(
  trace: TraceView,
  path: string,
): { covered: number; total: number } {
  const headings = trace.headings.filter((heading) => heading.path === path);
  return {
    covered: headings.filter((heading) => heading.covered_by.length > 0).length,
    total: headings.length,
  };
}

// ---- What a plan implements ------------------------------------------------------------------

export interface ImplementedHeading {
  heading: TraceHeading;
  /** The section document holding the heading, for its number, title and settle state. */
  section: TraceDoc | undefined;
}

/** The spec headings a plan, or one of its stages, covers: not the metaplan entry alone. */
export function implementedHeadings(trace: TraceView, planPath: string): ImplementedHeading[] {
  return trace.headings
    .filter((heading) =>
      heading.covered_by.some((cover) => cover.kind !== "entry" && cover.path === planPath),
    )
    .map((heading) => ({ heading, section: docOfPath(trace, heading.path) }));
}

// ---- Points that settling leaves open --------------------------------------------------------

export interface SettlePoint {
  kind: "call" | "question";
  /** The line as plain text, without its marker, clipped to one short quote. */
  quote: string;
  /** The heading the line sits under; null when it sits above every heading. */
  where: { title: string; anchor: string } | null;
  /** 1-based position among the open questions; 0 for a My call. */
  index: number;
}

type DocText = Pick<DocView, "text" | "sections">;

const MAX_QUOTE = 160;
const MARKER = /\s*\(my call[^)]*\)\.?/gi;
const LIST_ITEM = /^\s*(?:[-*+]|\d+[.)])\s+(.*\S)/;
const FENCE = /^\s*(?:```|~~~)/;

function plain(line: string): string {
  const text = line
    .replace(/^\s*(?:#{1,6}|[-*+]|\d+[.)]|>)\s+/, "")
    .replace(MARKER, "")
    .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[*`]/g, "")
    .replace(/\s+/g, " ")
    .trim();
  return text.length > MAX_QUOTE ? `${text.slice(0, MAX_QUOTE).trimEnd()}…` : text;
}

function sectionAt(doc: DocText, line: number): SettlePoint["where"] {
  const holding = doc.sections.filter(
    (section) => section.start_line <= line && line <= section.end_line,
  );
  const section = holding.at(-1);
  return section ? { title: section.title, anchor: section.anchor } : null;
}

/** Lines outside code fences, with their 1-based numbers. */
function proseLines(text: string): { line: number; text: string }[] {
  let fenced = false;
  const lines: { line: number; text: string }[] = [];
  text.split("\n").forEach((raw, index) => {
    if (FENCE.test(raw)) fenced = !fenced;
    else if (!fenced) lines.push({ line: index + 1, text: raw });
  });
  return lines;
}

/**
 * The My call lines and the open questions a document holds, in reading order, as the settle
 * confirmation quotes them. The server counts the same two things; this reads their text.
 */
export function settlePoints(doc: DocText): SettlePoint[] {
  const lines = proseLines(doc.text);
  const calls: SettlePoint[] = lines
    .filter(({ text }) => /\(my call/i.test(text))
    .map(({ line, text }) => ({
      kind: "call",
      quote: plain(text),
      where: sectionAt(doc, line),
      index: 0,
    }));
  const asked = doc.sections.filter((section) => /^open questions?$/i.test(section.title.trim()));
  const questions: SettlePoint[] = asked.flatMap((section) =>
    lines
      .filter(({ line }) => line > section.start_line && line <= section.end_line)
      .flatMap(({ text }) => LIST_ITEM.exec(text)?.[1] ?? [])
      .map((item) => ({
        kind: "question" as const,
        quote: plain(item),
        where: { title: section.title, anchor: section.anchor },
        index: 0,
      })),
  );
  return [...calls, ...questions.map((point, index) => ({ ...point, index: index + 1 }))];
}

// ---- Pressure record -------------------------------------------------------------------------

/** The text of the plan's pressure-test record dated `date`, from its `Pressure tests` section. */
export function pressureRecordText(doc: DocText, date: string): string | null {
  const section = doc.sections.find((item) => item.level === 2 && item.title === "Pressure tests");
  if (!section) return null;
  const prefix = `- ${date}:`;
  const record = proseLines(doc.text).find(
    ({ line, text }) =>
      line > section.start_line && line <= section.end_line && text.startsWith(prefix),
  );
  const body = record?.text.slice(prefix.length).trim();
  return body ? body : null;
}
