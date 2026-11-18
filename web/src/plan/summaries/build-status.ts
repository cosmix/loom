import type { SummaryPanel } from "@/plan/api/schema-summaries";

/**
 * Where the build of a page's summaries stands, from the live summary panel:
 * - `running`: a summary session holds one of the ids;
 * - `queued`: one is queued to start (behind the running session, or while an authoring turn or a
 *   pressure run holds starts), with the last failed start;
 * - `waiting`: a background rebuild waits for edits to its sources to stop;
 * - `asking`: one is in the large-rebuild question;
 * - `failed`: the last build failed, with its error;
 * - `idle`: nothing builds or will build without a press.
 */
export type BuildStatus =
  | { kind: "running"; done: number; total: number }
  | { kind: "queued"; behind: boolean; startError: string | null }
  | { kind: "waiting" }
  | { kind: "asking" }
  | { kind: "failed"; error: string }
  | { kind: "idle" };

/** Every summary the panel lists as out of date: queued, waiting, asked about, failed or incomplete. */
export function listedOutOfDate(panel: SummaryPanel | null): Set<string> {
  if (panel === null) return new Set();
  const failed = panel.failed.map(({ id }) => id);
  const asked = panel.question?.ids ?? [];
  return new Set([...panel.queue, ...panel.waiting, ...panel.incomplete, ...failed, ...asked]);
}

/** The status of `ids` taken together, the most advanced first; null before the panel loads. */
export function buildStatusOf(
  panel: SummaryPanel | null,
  ids: readonly string[],
): BuildStatus | null {
  if (panel === null) return null;
  const has = (list: readonly string[]) => ids.some((id) => list.includes(id));
  const { running } = panel;
  if (running && has(running.ids)) {
    return { kind: "running", done: running.done, total: running.total };
  }
  const waiting = panel.waiting;
  const starting = panel.queue.filter((id) => !waiting.includes(id));
  const asked = panel.question?.ids ?? [];
  if (has(starting) && !has(asked)) {
    return {
      kind: "queued",
      behind: running !== null,
      startError: panel.start_error,
    };
  }
  if (has(asked)) return { kind: "asking" };
  if (has(waiting)) return { kind: "waiting" };
  const failed = panel.failed.find((one) => ids.includes(one.id));
  if (failed) return { kind: "failed", error: failed.error };
  return { kind: "idle" };
}
