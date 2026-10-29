import { useAtomValue } from "jotai/react";

import type { Item, SessionView } from "@/plan/api/schema-chat";
import type { PressureRunView } from "@/plan/api/schema-pressure";
import type { RunStateView, RunStrip } from "@/plan/api/schema-run";
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
  runStrip: Pick<RunStrip, "daemon_running" | "plan"> | null;
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
): Activity {
  return { id, kind, title: ACTIVITY_LABELS[kind], subject, since };
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
  const activities = [activity(`pressure:${id}`, "pressure", run.plan, run.started_ms)];
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
    return [activity(`permission:${session.id}`, "permission", subject, permissionSince)];
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
      ),
    );
  }
  const running = input.runStrip?.daemon_running ?? input.runState?.workspace === "running";
  if (running) {
    activities.push(
      activity("run", "running", input.runStrip?.plan ?? input.runState?.active_plan ?? null, null),
    );
  }
  return activities.sort(compareActivities);
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
