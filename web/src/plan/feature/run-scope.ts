import type { ChecklistView } from "@/plan/api/schema";
import type { RunStateView, RunStrip } from "@/plan/api/schema-run";
import { offerOfKind, type PlanRow } from "@/plan/feature/page-offer";

/** Plans that count toward "every plan is done": a retired plan is out of the build. */
export function livePlans(checklist: ChecklistView): PlanRow[] {
  return checklist.plan_rows.filter((row) => row.state !== "retired");
}

/** Every live plan is done, and there is at least one. */
export function allPlansDone(checklist: ChecklistView): boolean {
  const live = livePlans(checklist);
  return live.length > 0 && live.every((row) => row.state === "done");
}

/**
 * The plan the page's one Start a run button targets: the checklist's start_run offer when it
 * names a written plan of this feature, else the run dialog's preselected plan when that is a
 * written plan here, else the first written plan. Null when no plan is written.
 */
function startTarget(checklist: ChecklistView, preselected: string | null): string | null {
  const written = checklist.plan_rows.filter((row) => row.state === "written");
  const isWritten = (path: string | null | undefined): path is string =>
    written.some((row) => row.path === path);
  const offered = offerOfKind(checklist, "start_run")?.target;
  if (isWritten(offered)) return offered;
  if (isWritten(preselected)) return preselected;
  return written[0]?.path ?? null;
}

type RunScope = "idle" | "own" | "foreign";

/**
 * The run state covers only the workspace's one active plan, so a run in flight for another
 * feature must not read as this feature's: `own` only when the active plan is a row of this
 * feature, `foreign` when some other plan holds the workspace.
 */
export function runScope(checklist: ChecklistView, state: RunStateView | null): RunScope {
  const active = state?.active_plan ?? null;
  if (state === null || active === null || state.workspace === "absent") return "idle";
  if (state.workspace === "done") return "idle";
  return checklist.plan_rows.some((row) => row.path === active) ? "own" : "foreign";
}

/** How the page's run-state request stands: asked, answered with a state, or failed. */
export type RunRead = "pending" | "read" | "failed";

const HELD = "A run holds the workspace.";
const UNREAD = "Run state unavailable.";

/** The page's one Start a run button: the plan it opens the run dialog on, and why it is blocked. */
type StartControl = { path: string | null; reason: string | null };

/**
 * The page's Start a run button. A run of this feature that stopped resumes through it, targeting
 * its own plan even when no plan is written. A run in flight, a foreign stopped run (the server
 * refuses both, `run/start.rs` admit) and an unreadable state draw it blocked with one reason.
 * Otherwise it starts `startTarget`. It is drawn whenever there is a plan to start, a run to
 * resume or a start_run primary.
 * Null when none of those holds.
 */
export function startControl(
  checklist: ChecklistView,
  state: RunStateView | null,
  read: RunRead,
): StartControl | null {
  const offer = checklist.primary?.kind === "start_run" ? checklist.primary : null;
  const target = startTarget(checklist, state?.preselected ?? null);
  const path = target ?? offer?.target ?? null;
  const drawn = path !== null || offer !== null;
  if (state === null) {
    if (!drawn) return null;
    return { path, reason: read === "failed" ? UNREAD : null };
  }
  const scope = runScope(checklist, state);
  if (state.workspace === "stopped" && scope === "own") {
    return { path: state.active_plan, reason: null };
  }
  if (!drawn) return null;
  const held = state.workspace === "running" || state.workspace === "stopped";
  return { path, reason: held ? HELD : null };
}

/**
 * What an idle scope says when no run is active: plain, or naming plan files left reading
 * "running" (a plan file left IN_PROGRESS), which no run backs.
 */
function idleLine(checklist: ChecklistView): string {
  const running = checklist.plan_rows.filter((row) => row.state === "running");
  if (running.length === 0) return "Nothing is running.";
  const still =
    running.length === 1
      ? `${running[0]!.slug} still reads as running.`
      : `${running.length} plans still read as running.`;
  return `No run is active here. ${still}`;
}

/**
 * The page's one line about the run. Without a state the line follows the request: "checking"
 * while it is out, "could not read" once it failed. Every plan done is the checklist's own fact
 * and reads either way.
 */
export function runSentence(
  checklist: ChecklistView,
  state: RunStateView | null,
  strip: RunStrip | null,
  read: RunRead,
): string {
  const done = allPlansDone(checklist);
  const doneLine = "Every plan is done.";
  if (state === null) {
    if (done) return doneLine;
    return read === "failed" ? "Could not read the run state." : "Checking for a run.";
  }
  const scope = runScope(checklist, state);
  const row = checklist.plan_rows.find((candidate) => candidate.path === state.active_plan);
  if (scope === "foreign") return "Another plan holds the workspace.";
  if (scope === "own" && row) {
    if (state.workspace === "stopped") return `The run of ${row.slug} is stopped.`;
    if (state.workspace === "initialized") return `The run of ${row.slug} is set up, not started.`;
    const progress =
      strip?.plan === row.path && strip.total > 0
        ? `: ${strip.merged} of ${strip.total} stages merged`
        : "";
    return `${row.slug} is running${progress}.`;
  }
  return done ? doneLine : idleLine(checklist);
}
