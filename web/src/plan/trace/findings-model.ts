import type { OfferView } from "@/plan/api/schema";
import type { SessionView } from "@/plan/api/schema-chat";
import { isActiveSession } from "@/plan/chat/session-home";
import type { NodeRef, TraceFinding, TraceView } from "@/plan/api/schema-trace";
import { fileName } from "@/plan/concept/passages";
import { docOfPath, shortSlug } from "@/plan/trace/model";
import { plural } from "@/plan/ui/plural";

// Pure reads of the findings ledger as the trace carries it: the board's lanes, counts, About
// labels, the state each card shows and the feature's Address the findings offer. Finding text is
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

/** The board's lanes, in the order a finding moves: verified, decided by the reader, fixed. */
export type LaneKey = "verify" | "decide" | "addressed" | "aside";

export const LANE_KEYS: readonly LaneKey[] = ["verify", "decide", "addressed", "aside"];

/** The lane a card state belongs to: what the finding needs from the reader. */
export function laneOf(state: CardState): LaneKey {
  switch (state) {
    case "unverified":
    case "stale":
      return "verify";
    case "open":
    case "addressing":
    case "unaddressed":
      return "decide";
    case "addressed":
      return "addressed";
    case "dismissed":
    case "duplicate":
      return "aside";
  }
}

export interface Lane {
  key: LaneKey;
  entries: TraceFinding[];
}

const SEVERITY_RANK = { high: 0, medium: 1, low: 2 } as const;

/**
 * The decide lane's order: findings an address run could not fix last, then by severity (high
 * first, none last). Array sort is stable, so ledger order holds inside a rank.
 */
function decideOrder(a: TraceFinding, b: TraceFinding): number {
  const failed = Number(a.finding.status.kind === "unaddressed");
  const failedB = Number(b.finding.status.kind === "unaddressed");
  const rank = (e: TraceFinding) =>
    e.finding.severity === null ? 3 : SEVERITY_RANK[e.finding.severity];
  return failed - failedB || rank(a) - rank(b);
}

/** Every finding in its lane, the four lanes always present; other lanes keep ledger order. */
export function boardLanes(findings: TraceFinding[], addressing: boolean): Lane[] {
  return LANE_KEYS.map((key) => {
    const entries = findings.filter((entry) => laneOf(cardState(entry, addressing)) === key);
    return { key, entries: key === "decide" ? entries.sort(decideOrder) : entries };
  });
}

/** Whether the ledger holds findings and none of them waits on the reader (to verify or decide). */
export function nothingWaits(findings: TraceFinding[]): boolean {
  return (
    findings.length > 0 &&
    boardLanes(findings, false).every(
      ({ key, entries }) => entries.length === 0 || (key !== "verify" && key !== "decide"),
    )
  );
}

/** The words a card state reads as; a duplicate adds the finding it repeats where it can. */
export function stateWord(entry: TraceFinding, state: CardState): string {
  switch (state) {
    case "open":
      return "Open";
    case "addressing":
      return "Being addressed";
    case "unverified":
      return "Unverified";
    case "stale":
      return "Stale";
    case "duplicate":
      return `Duplicate of ${entry.check.duplicate_of ?? "an earlier finding"}`;
    case "addressed":
      return "Addressed";
    case "unaddressed":
      return "Could not address";
    case "dismissed":
      return "Dismissed";
  }
}

/** What a card's state adds to its word: the ledger's reason, or null where the word says it all. */
export function stateReason(entry: TraceFinding, state: CardState): string | null {
  const detail = entry.finding.status.detail;
  switch (state) {
    case "unverified":
      return "No quote checked against the documents yet";
    case "stale":
      return "The quote is no longer in the passage it is about";
    case "unaddressed":
    case "dismissed":
      return detail;
    case "addressed":
      return `Changed ${plural(entry.changed.length, "place")}`;
    default:
      return null;
  }
}

/** Where a finding came from, in the words of a card (`short`) and of its dialog (`words`). */
export function findingSource(
  source: string | null,
  feature: string,
): { run: string | null; short: string; words: string } {
  const run = runOf(source);
  if (run !== null) {
    const name = shortRun(run, feature);
    const stage = /\(([^,)]+),/.exec(source ?? "")?.[1];
    return {
      run: name,
      short: `run ${name}`,
      words:
        stage === undefined ? `Run review of ${name}` : `Run review of ${name}, stage ${stage}`,
    };
  }
  const dated = /^review (\d{4}-\d{2}-\d{2})$/.exec(source ?? "")?.[1];
  return {
    run: null,
    short: "your review",
    words:
      dated === undefined ? "Your review of the feature" : `Your review of the feature, ${dated}`,
  };
}

/** The ledger in one line: `12 findings from 2 run reviews and your reviews`. */
export function ledgerSummary(findings: TraceFinding[]): string {
  const groups = groupFindings(findings);
  const runs = groups.filter((group) => group.run !== null).length;
  const sources = [
    ...(runs > 0 ? [plural(runs, "run review")] : []),
    ...(groups.length > runs ? ["your reviews"] : []),
  ];
  return `${plural(findings.length, "finding")} from ${sources.join(" and ")}`;
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
