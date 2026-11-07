import { apiPost, type ApiResult } from "@/plan/api/client";
import { saveResultSchema, type SaveResult } from "@/plan/api/schema-files";
import type { Finding, FindingKind, TraceEntry } from "@/plan/api/schema-trace";

// What the Findings page does with a finding: the seeds it hands to Plan follow-ups and the
// status change it posts. The ledger text is agent-written; seeds carry it as the user's own
// first message, which the session treats like anything else the user typed.

const aboutTokens = (finding: Finding): string => finding.about.join(", ");

/** The first message of a Plan follow-ups session that plans one finding. */
export function planSeed(finding: Finding): string {
  return `Plan finding ${finding.id}: ${finding.title}\nAbout: ${aboutTokens(finding)}\n\n${finding.text}`;
}

/** The first message of a Plan follow-ups session that folds one finding into a metaplan entry. */
export function foldSeed(finding: Finding, entry: TraceEntry): string {
  return (
    `Fold finding ${finding.id} into metaplan entry ${entry.title} (metaplan#${entry.anchor}): ` +
    `${finding.title}\nAbout: ${aboutTokens(finding)}`
  );
}

/** The ledger's repository path. */
export function ledgerPath(feature: string): string {
  return `doc/plans/${feature}/findings.md`;
}

/** The longest dismissal reason the route takes. */
export const REASON_MAX = 200;

/**
 * A dismissal reason as the route takes it: one line (line breaks and tabs become spaces, as the
 * server does), other control characters removed, trimmed, at most `REASON_MAX` characters, and
 * never empty.
 */
export function dismissReason(input: string): string | null {
  const reason = input
    .replace(/[\n\r\t]/g, " ")
    .replace(/\p{Cc}/gu, "")
    .trim();
  const cut = [...reason].slice(0, REASON_MAX).join("").trim();
  return cut === "" ? null : cut;
}

/** What a failed or refused status change tells the reader; null when the ledger was written. */
export function failureOf(result: ApiResult<SaveResult>): string | null {
  if (!result.ok) return result.message;
  if (result.value.result === "saved") return null;
  return result.value.result === "conflict"
    ? "The findings file changed while you were reading it. The page has reloaded; try again."
    : "The findings file did not accept the change.";
}

/**
 * Rewrites one finding's `Status:` line. `hash` is the ledger hash read with the trace the page
 * shows (`useLedgerHash`), so a ledger edited since that trace loaded is refused by the server as
 * a conflict rather than overwritten.
 */
export function setFindingStatus(
  feature: string,
  id: string,
  hash: string,
  status: { kind: FindingKind; detail: string },
): Promise<ApiResult<SaveResult>> {
  return apiPost("/api/findings/status", { feature, id, hash, status }, saveResultSchema, {
    contentType: "json",
    resultStatuses: [409, 422],
  });
}
