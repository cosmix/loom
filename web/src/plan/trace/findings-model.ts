import type { OfferView } from "@/plan/api/schema";
import type { SessionView } from "@/plan/api/schema-chat";
import { isActiveSession } from "@/plan/chat/session-home";
import {
  FINDING_KINDS,
  type NodeRef,
  type TraceFinding,
  type TraceView,
} from "@/plan/api/schema-trace";
import { fileName } from "@/plan/concept/passages";
import { docOfPath, shortSlug } from "@/plan/trace/model";

// Pure reads of the findings ledger as the trace carries it: grouping, counts, About labels, the
// state each card shows and the feature's Address the findings offer. Finding text is
// agent-written; callers render the ledger's strings as text (`Untrusted`).

/** The group of findings that came from no run review: a person's own review. */
export const REVIEWS_LABEL = "Reviews";

/** The offer id the server starts an address run for (`Offer::AddressFindings`). */
export const ADDRESS_FINDINGS_ID = "address-findings";

export interface FindingGroup {
  /** The run review's stem (`REVIEW-PLAN-x`), or `Reviews`. */
  label: string;
  /** Null for the Reviews group. */
  run: string | null;
  findings: TraceFinding[];
}

/** Open and verified: what the page counts and an address run takes (`TraceFinding::actionable`). */
export function isActionable({ finding, check }: TraceFinding): boolean {
  return finding.status.kind === "open" && check.kind === "verified";
}

/** The findings an address run would take. */
export function actionableFindings(trace: TraceView): TraceFinding[] {
  return trace.findings.filter(isActionable);
}

/** The number the Run panel's Findings link and the spine show. */
export function openCount(trace: TraceView): number {
  return actionableFindings(trace).length;
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

/**
 * The header's tally: `3 to address · 2 to verify · 1 addressed · 1 dismissed`, leaving out zero
 * counts. Open findings split by their check: verified ones are to address, unverified and stale
 * ones to verify (a review of the feature does that), and duplicates count nowhere.
 */
export function tally(findings: TraceFinding[]): string {
  const open = findings.filter(({ finding }) => finding.status.kind === "open");
  const counts: Array<[number, string]> = [
    [open.filter(isActionable).length, "to address"],
    [
      open.filter(({ check }) => check.kind === "unverified" || check.kind === "stale").length,
      "to verify",
    ],
    ...FINDING_KINDS.filter((kind) => kind !== "open").map((kind): [number, string] => [
      findings.filter(({ finding }) => finding.status.kind === kind).length,
      kind,
    ]),
  ];
  return counts
    .filter(([count]) => count > 0)
    .map(([count, word]) => `${count} ${word}`)
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

/**
 * What a card shows. An open finding reads its check (an actionable one reads `addressing` while
 * this feature's address run is live); any other reads its status.
 */
export type CardState =
  | "open"
  | "addressing"
  | "unverified"
  | "stale"
  | "duplicate"
  | "addressed"
  | "unaddressed"
  | "dismissed";

export function cardState(entry: TraceFinding, addressing: boolean): CardState {
  const { status } = entry.finding;
  if (status.kind !== "open") return status.kind;
  if (entry.check.kind !== "verified") return entry.check.kind;
  return addressing ? "addressing" : "open";
}

/** Whether a card draws as a full card (text, evidence, actions) rather than one line. */
export function isFullCard(state: CardState): boolean {
  return state === "open" || state === "addressing" || state === "unverified" || state === "stale";
}

/** The words a card's state reads as; `severity` and `detail` come from the ledger. */
export function stateText(entry: TraceFinding, state: CardState): string {
  const detail = entry.finding.status.detail ?? "";
  const severity = entry.finding.severity;
  switch (state) {
    case "open":
      return severity === null ? "Open" : `Open · ${severity}`;
    case "addressing":
      return "Being addressed";
    case "unverified":
      return "Unverified: no quote checked against the documents yet";
    case "stale":
      return "Stale: the quote is no longer in the passage it is about";
    case "duplicate":
      return `Duplicate of ${entry.check.duplicate_of ?? "an earlier finding"}`;
    case "addressed":
      return "Addressed";
    case "unaddressed":
      return `Could not address: ${detail}`;
    case "dismissed":
      return `Dismissed: ${detail}`;
  }
}

/**
 * Whether this feature's address run is live: the open session started the Address the findings
 * offer for this feature.
 */
export function addressRunLive(session: SessionView | null, feature: string): boolean {
  if (!isActiveSession(session)) return false;
  return session.offer === ADDRESS_FINDINGS_ID && session.feature === feature;
}

/** The feature's Address the findings offer, undefined while no finding is actionable. */
export function addressOffer(trace: TraceView, feature: string): OfferView | undefined {
  if (openCount(trace) === 0) return undefined;
  return {
    id: ADDRESS_FINDINGS_ID,
    kind: "address_findings",
    label: "Address the findings",
    target: null,
    feature,
  };
}

/** A plan stage token (`stage:build`) read as `<short slug> › build`; null for any other node. */
function stageLabel(trace: TraceView, feature: string, node: NodeRef): string | null {
  const stage = /^stage:(.+)$/.exec(node.anchor ?? "")?.[1];
  const doc = docOfPath(trace, node.path);
  if (stage === undefined || doc?.role !== "plan") return null;
  const slug = doc.slug ?? shortRun(fileName(node.path).replace(/\.md$/, ""), feature);
  return `${shortSlug(feature, slug)} › ${stage}`;
}

/** What an About chip reads: the heading, entry, plan stage or document the node names. */
export function nodeLabel(trace: TraceView, node: NodeRef, feature: string): string {
  const fallback = fileName(node.path);
  if (node.anchor === null) return docOfPath(trace, node.path)?.title ?? fallback;
  const heading = trace.headings.find((h) => h.path === node.path && h.anchor === node.anchor);
  if (heading) return heading.title;
  const stage = stageLabel(trace, feature, node);
  if (stage !== null) return stage;
  const doc = docOfPath(trace, node.path);
  const entry =
    doc?.role === "metaplan" ? trace.entries.find((e) => e.anchor === node.anchor) : undefined;
  return entry?.title ?? `${doc?.title ?? fallback}#${node.anchor}`;
}
