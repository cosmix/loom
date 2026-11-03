import { useAtomValue } from "jotai/react";

import type { Item, SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import type { RunStateView, RunStrip as Strip } from "@/plan/api/schema-run";
import type { SummaryPanel } from "@/plan/api/schema-summaries";
import { pendingPermissionsAtom, sessionAtom } from "@/plan/chat/model";
import { pressureRunAtom } from "@/plan/chat/pressure/use-pressure";
import { runStateAtom, runStripAtom } from "@/plan/run/use-run";
import { summaryPanelAtom } from "@/plan/summaries/state";
import { ACTIVITY_KINDS, ACTIVITY_LABELS, type ActivityKind } from "@/plan/ui/activity-glyphs";

export interface Activity {
  id: string;
  kind: ActivityKind;
  title: string;
  subject: string | null;
  since: number | null; // Epoch milliseconds; null when the operation's start is unknown.
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
  const activities = [activity(`pressure:${id}`, "pressure", run.plan, run.started_ms, title)];
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

function sessionActivities(
  session: SessionView | null,
  pending: ActivitiesInput["pendingPermissions"],
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
    return [activity(`waiting:${session.id}`, "waiting", session.name, since)];
  }
  // The ending state explicitly writes final notes; working alone does not imply a draft.
  if (session.state === "ending" && session.kind === "authoring") {
    return [activity(`drafting:${session.id}`, "drafting", session.name, since)];
  }
  return [];
}

export function deriveActivities(input: ActivitiesInput): Activity[] {
  const activities = [
    ...pressureActivities(input.pressureRun),
    ...sessionActivities(input.session, input.pendingPermissions),
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

export function useActivities(): Activity[] {
  const summaryPanel = useAtomValue(summaryPanelAtom);
  const runState = useAtomValue(runStateAtom);
  const runStrip = useAtomValue(runStripAtom);
  const pressureRun = useAtomValue(pressureRunAtom);
  const session = useAtomValue(sessionAtom);
  const pendingPermissions = useAtomValue(pendingPermissionsAtom);
  return deriveActivities({
    summaryPanel,
    runState,
    runStrip,
    pressureRun,
    session,
    pendingPermissions,
  });
}

export function mostUrgent(activities: Activity[]): Activity | null {
  return activities.reduce<Activity | null>(
    (urgent, candidate) =>
      urgent === null || compareActivities(candidate, urgent) < 0 ? candidate : urgent,
    null,
  );
}
