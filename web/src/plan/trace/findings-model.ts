import type { OfferView } from "@/plan/api/schema";
import {
  FINDING_KINDS,
  type FindingStatus,
  type NodeRef,
  type TraceEntry,
  type TraceFinding,
  type TraceView,
} from "@/plan/api/schema-trace";
import { fileName } from "@/plan/concept/passages";
import { PLAN_FOLLOW_UPS_ID } from "@/plan/new/follow-up-features";
import { docOfPath } from "@/plan/trace/model";

// Pure reads of the findings ledger as the trace carries it: grouping, counts, About labels and
// which metaplan entries a finding may be folded into. Finding text is agent-written; callers
// render every string here as text.

/** The group of findings that came from no run review: a person's own review. */
export const REVIEWS_LABEL = "Reviews";

export interface FindingGroup {
  /** The run review's stem (`REVIEW-PLAN-x`), or `Reviews`. */
  label: string;
  /** Null for the Reviews group. */
  run: string | null;
  findings: TraceFinding[];
}

/** Findings still waiting for a decision. */
export function openFindings(trace: TraceView): TraceFinding[] {
  return trace.findings.filter(({ finding }) => finding.status.kind === "open");
}

/** The number the Run panel's Findings link shows. */
export function openCount(trace: TraceView): number {
  return openFindings(trace).length;
}

/**
 * The run review a `Source:` line names (`REVIEW-PLAN-x.md#open-findings (stage, hash)` gives
 * `REVIEW-PLAN-x`); null for no source or a source that is not a run review (`review 2026-11-06`).
 */
export function runOf(source: string | null): string | null {
  const file = /^(REVIEW-[^#\s]+?)(?:\.md)?(?:#|\s|$)/.exec(source ?? "")?.[1];
  return file ?? null;
}

/**
 * What a run review's stem reads as on the page: `REVIEW-PLAN-feat-x-03-summaries` in feature
 * `feat-x` gives `03-summaries`; a plan's lifecycle prefix (`DONE-`) comes off with `PLAN-`.
 */
export function shortRun(run: string, feature: string): string {
  const stem = run.replace(/^REVIEW-/, "").replace(/^(?:(?:IN_PROGRESS|DONE|RETIRED)-)?PLAN-/, "");
  return stem.startsWith(`${feature}-`) ? stem.slice(feature.length + 1) : stem;
}

/** A group's heading: `Run of 03-summaries`, or `Reviews`. */
export function groupTitle(group: FindingGroup, feature: string): string {
  return group.run === null ? REVIEWS_LABEL : `Run of ${shortRun(group.run, feature)}`;
}

/** The header's tally: `3 open · 2 planned · 1 folded · 1 dismissed`, leaving out zero counts. */
export function tally(findings: TraceFinding[]): string {
  return FINDING_KINDS.map((kind) => ({
    kind,
    count: findings.filter(({ finding }) => finding.status.kind === kind).length,
  }))
    .filter(({ count }) => count > 0)
    .map(({ kind, count }) => `${count} ${kind}`)
    .join(" · ");
}

/**
 * Findings grouped by run review, newest run first, then the Reviews group. A run is newer when
 * its first finding sits later in the ledger, which an import appends to; the `Source:` line has
 * no date. Findings keep their file order inside a group.
 */
export function groupFindings(findings: TraceFinding[]): FindingGroup[] {
  const runs = new Map<string, { first: number; findings: TraceFinding[] }>();
  const reviews: TraceFinding[] = [];
  findings.forEach((entry, index) => {
    const run = runOf(entry.finding.source);
    if (run === null) {
      reviews.push(entry);
      return;
    }
    const group = runs.get(run) ?? { first: index, findings: [] };
    group.findings.push(entry);
    runs.set(run, group);
  });
  const groups: FindingGroup[] = [...runs]
    .sort(([, a], [, b]) => b.first - a.first)
    .map(([run, group]) => ({ label: run, run, findings: group.findings }));
  if (reviews.length > 0) groups.push({ label: REVIEWS_LABEL, run: null, findings: reviews });
  return groups;
}

/** A plan file under a lifecycle prefix has started running, is done or was retired. */
const STARTED_PLAN = /^(?:IN_PROGRESS|DONE|RETIRED)-/;

/** The entries a finding can still be folded into: no plan yet, or a plan that has not started. */
export function foldableEntries(trace: TraceView): TraceEntry[] {
  return trace.entries.filter((entry) => {
    if (entry.plan === null) return true;
    return !STARTED_PLAN.test(fileName(entry.plan));
  });
}

export interface FoldChoices {
  /** The entries a finding may be folded into, the suggested one first. */
  entries: TraceEntry[];
  /** The entry covering most of what the finding is about; null when none covers any of it. */
  suggested: TraceEntry | null;
  /** The entries not offered because their plan is running, done or retired. */
  leftOut: TraceEntry[];
}

/** An entry covers a node when it names the node's file whole or the node's own heading. */
function covers(entry: TraceEntry, node: NodeRef): boolean {
  return entry.covers.some(
    (cover) => cover.path === node.path && (cover.anchor === null || cover.anchor === node.anchor),
  );
}

/** The picker's entries for a finding about `about`: the best match first, then file order. */
export function foldChoices(trace: TraceView, about: NodeRef[]): FoldChoices {
  const open = foldableEntries(trace);
  const score = (entry: TraceEntry) => about.filter((node) => covers(entry, node)).length;
  const best = open.reduce<TraceEntry | null>(
    (top, entry) => (score(entry) > (top === null ? 0 : score(top)) ? entry : top),
    null,
  );
  return {
    entries: best === null ? open : [best, ...open.filter((entry) => entry !== best)],
    suggested: best,
    leftOut: trace.entries.filter((entry) => !open.includes(entry)),
  };
}

/** The words a status reads as on a finding card. */
export function statusText(status: FindingStatus): string {
  const detail = status.detail ?? "";
  switch (status.kind) {
    case "open":
      return "Open";
    case "planned":
      return `Planned in ${detail}`;
    case "folded":
      return `Folded into ${detail}`;
    case "dismissed":
      return `Dismissed: ${detail}`;
  }
}

/** What an About chip reads: the heading, entry or document the node names. */
export function nodeLabel(trace: TraceView, node: NodeRef): string {
  const fallback = fileName(node.path);
  if (node.anchor === null) return docOfPath(trace, node.path)?.title ?? fallback;
  const heading = trace.headings.find((h) => h.path === node.path && h.anchor === node.anchor);
  if (heading) return heading.title;
  const doc = docOfPath(trace, node.path);
  const entry =
    doc?.role === "metaplan" ? trace.entries.find((e) => e.anchor === node.anchor) : undefined;
  return entry?.title ?? `${doc?.title ?? fallback}#${node.anchor}`;
}

/**
 * The Plan follow-ups offer for this feature. The server starts `plan-follow-ups` for any feature
 * that has a metaplan, while the checklist carries the offer only once every plan is done, so the
 * checklist's own offer wins and otherwise one is built from the trace; undefined without a metaplan.
 */
export function followUpsOffer(
  checklistOffer: OfferView | undefined,
  trace: TraceView,
  feature: string,
): OfferView | undefined {
  if (checklistOffer !== undefined) return checklistOffer;
  if (!trace.documents.some((doc) => doc.role === "metaplan")) return undefined;
  return {
    id: PLAN_FOLLOW_UPS_ID,
    kind: "plan_follow_ups",
    label: "Plan follow-ups",
    target: null,
    feature,
  };
}
