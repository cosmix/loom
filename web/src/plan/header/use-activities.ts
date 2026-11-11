import { useAtomValue } from "jotai/react";

import type { BackgroundTaskView } from "@/plan/api/schema-background";
import type { Item, SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import type { RunStateView, RunStrip as Strip } from "@/plan/api/schema-run";
import type { SummaryPanel } from "@/plan/api/schema-summaries";
import { chatBackgroundAtom, isUnended } from "@/plan/chat/background-tasks";
import { pendingPermissionsAtom, sessionAtom } from "@/plan/chat/model";
import { planFileName, pressureProgress, stepLabel } from "@/plan/chat/pressure/steps";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { runStateAtom, runStripAtom } from "@/plan/run/use-run";
import { kickoffAtom, kickoffDraftsAtom } from "@/plan/state/atoms";
import { summaryPanelAtom } from "@/plan/summaries/state";
import { plural } from "@/plan/ui/plural";
import { ACTIVITY_KINDS, ACTIVITY_LABELS, type ActivityKind } from "@/plan/ui/activity-glyphs";

export interface Activity {
  id: string;
  kind: ActivityKind;
  title: string;
  subject: string | null;
  since: number | null; // Epoch milliseconds; null when the operation's start is unknown.
  /** Counted work (a pressure run's finished steps); its row draws a determinate bar. */
  progress?: { done: number; total: number };
  /**
   * The chat a waiting row stands for; its row links to that chat's screen. `named` is false while
   * the title is the state's own words, which the live row replaces with the session brief's label.
   */
  session?: { id: string; kickoff: boolean; named: boolean };
}

type Permission = Extract<Item, { type: "permission" }>;

export interface ActivitiesInput {
  summaryPanel: Pick<SummaryPanel, "running"> | null;
  runState: Pick<RunStateView, "workspace" | "active_plan"> | null;
  runStrip:
    | (Pick<Strip, "daemon_running" | "plan"> & Partial<Pick<Strip, "merged" | "total">>)
    | null;
  pressureRun: PressureRunView | null;
  session: SessionView | null;
  pendingPermissions: readonly Pick<Permission, "tool">[];
  /** The session's kickoff, with the name its draft or shape carries; null for any other chat. */
  kickoff?: { name: string | null } | null;
  /** The chat session's background tasks (`chatBackgroundAtom`); the table's own filter applies. */
  background?: readonly Pick<BackgroundTaskView, "state" | "started_at">[];
}

function timestamp(value: string): number | null {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function activity(
  id: string,
  kind: ActivityKind,
  subject: string | null,
  since: number | null,
  title: string = ACTIVITY_LABELS[kind],
): Activity {
  return { id, kind, title, subject, since };
}

function compareActivities(a: Activity, b: Activity): number {
  const urgency = ACTIVITY_KINDS.indexOf(a.kind) - ACTIVITY_KINDS.indexOf(b.kind);
  if (urgency !== 0) return urgency;
  if (a.since === b.since) return 0;
  // Unknown start times follow known ones within the same urgency.
  if (a.since === null) return 1;
  if (b.since === null) return -1;
  return a.since - b.since;
}

function pressureActivities(run: PressureRunView | null): Activity[] {
  if (!run || run.ended !== null) return [];
  const id = `${run.plan}:${run.started_ms}`;
  const title = `${ACTIVITY_LABELS.pressure} · round ${run.round} of ${run.rounds}`;
  const subject = `${planFileName(run.plan)} · ${stepLabel(run.step)}`;
  const row = activity(`pressure:${id}`, "pressure", subject, run.started_ms, title);
  const activities: Activity[] = [{ ...row, progress: pressureProgress(run) }];
  // Codex can be running alongside Claude before run.step switches to "codex".
  for (const step of run.steps) {
    if (
      step.kind === "codex" &&
      step.state === "running" &&
      (step.codex === null || step.codex.state === "running")
    ) {
      activities.push(activity(`codex:${id}:${step.round}`, "codex", run.plan, step.started_ms));
    }
  }
  return activities;
}

/** A waiting chat is named by its kickoff's name, never by the tmux label `session.name`. */
function waitingActivity(
  session: SessionView,
  since: number | null,
  kickoff: ActivitiesInput["kickoff"] = null,
): Activity {
  const label = ACTIVITY_LABELS.waiting;
  const name = kickoff?.name || null;
  const row = activity(`waiting:${session.id}`, "waiting", name && label, since, name ?? label);
  return { ...row, session: { id: session.id, kickoff: kickoff !== null, named: name !== null } };
}

function sessionActivities(
  session: SessionView | null,
  pending: ActivitiesInput["pendingPermissions"],
  kickoff: ActivitiesInput["kickoff"] = null,
): Activity[] {
  if (!session || session.state === "ended" || session.state === "failed") return [];
  const since = timestamp(session.state_since);
  if (session.state === "permission" || pending.length > 0) {
    const subject = pending.length > 0 ? pending.map((item) => item.tool).join(", ") : session.name;
    const permissionSince = session.state === "permission" ? since : null;
    return [
      activity(
        `permission:${session.id}`,
        "permission",
        subject,
        permissionSince,
        "Waiting for your answer",
      ),
    ];
  }
  if (session.state === "starting") {
    return [activity(`starting:${session.id}`, "starting", session.name, since)];
  }
  if (session.state === "idle" && session.kind !== "pressure") {
    return [waitingActivity(session, since, kickoff)];
  }
  // The ending state explicitly writes final notes; working alone does not imply a draft.
  if (session.state === "ending" && session.kind === "authoring") {
    return [activity(`drafting:${session.id}`, "drafting", session.name, since)];
  }
  return [];
}

/**
 * One row for the chat's background tasks that have not ended, in any session state: the user is
 * not waiting on the main agent, so the row is a notice, not progress (`inProgress` leaves it out).
 */
function backgroundActivities(
  session: SessionView | null,
  tasks: ActivitiesInput["background"] = [],
): Activity[] {
  const unended = tasks.filter((task) => isUnended(task.state));
  if (!session || unended.length === 0) return [];
  const starts = unended
    .map((task) => timestamp(task.started_at))
    .filter((ms): ms is number => ms !== null);
  const since = starts.length > 0 ? Math.min(...starts) : null;
  const title = plural(unended.length, "background task");
  return [activity(`background:${session.id}`, "background", null, since, title)];
}

/** What the pill counts as "in progress": every row but the background notice. */
export function inProgress(activities: readonly Activity[]): Activity[] {
  return activities.filter((entry) => entry.kind !== "background");
}

export function deriveActivities(input: ActivitiesInput): Activity[] {
  const activities = [
    ...pressureActivities(input.pressureRun),
    ...sessionActivities(input.session, input.pendingPermissions, input.kickoff),
    ...backgroundActivities(input.session, input.background),
  ];
  const rebuild = input.summaryPanel?.running;
  if (rebuild) {
    activities.push(
      activity(
        "summaries",
        "summaries",
        `${rebuild.done} of ${rebuild.total}`,
        timestamp(rebuild.started_at),
        `${rebuild.total} ${rebuild.total === 1 ? "summary" : "summaries"} rebuilding`,
      ),
    );
  }
  const running = input.runStrip?.daemon_running ?? input.runState?.workspace === "running";
  if (running) {
    activities.push(activity("run", "running", runSubject(input), null));
  }
  return activities.sort(compareActivities);
}

/** The plan, with merge progress once the run strip reports a stage count. */
function runSubject({ runStrip, runState }: ActivitiesInput): string | null {
  const path = runStrip?.plan ?? runState?.active_plan ?? null;
  // The file name keeps the progress visible when the row truncates.
  const plan = path?.split("/").at(-1) ?? path;
  const progress = runStrip ? stagesMerged(runStrip) : null;
  if (plan === null || progress === null) return plan;
  return `${plan} · ${progress}`;
}

/** "M of N stages merged" once the strip reports a stage count; no count is better than a guess. */
export function stagesMerged({
  merged,
  total,
}: Partial<Pick<Strip, "merged" | "total">>): string | null {
  if (merged === undefined || !total) return null;
  return `${merged} of ${total} stages merged`;
}

/** The session's kickoff: its draft first (it carries the user's rename), else the live shape. */
function useKickoffOf(session: SessionView | null): { name: string | null } | null {
  const drafts = useAtomValue(kickoffDraftsAtom);
  const view = useAtomValue(kickoffAtom);
  if (session === null) return null;
  const draft = drafts.find((item) => item.session === session.id);
  if (draft) return { name: draft.name };
  return view?.session === session.id ? { name: view.shape.name } : null;
}

export function useActivities(): Activity[] {
  const summaryPanel = useAtomValue(summaryPanelAtom);
  const runState = useAtomValue(runStateAtom);
  const runStrip = useAtomValue(runStripAtom);
  const pressureRun = useAtomValue(pressureRunAtom);
  const session = useAtomValue(sessionAtom);
  const pendingPermissions = useAtomValue(pendingPermissionsAtom);
  const kickoff = useKickoffOf(session);
  const background = useAtomValue(chatBackgroundAtom);
  return deriveActivities({
    summaryPanel,
    runState,
    runStrip,
    pressureRun,
    session,
    pendingPermissions,
    kickoff,
    background,
  });
}

export function mostUrgent(activities: Activity[]): Activity | null {
  return activities.reduce<Activity | null>(
    (urgent, candidate) =>
      urgent === null || compareActivities(candidate, urgent) < 0 ? candidate : urgent,
    null,
  );
}
