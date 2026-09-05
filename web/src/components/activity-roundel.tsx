import { useAtomValue } from "jotai/react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import type { StageSummary } from "@/api/schema";
import { snapshotAtom } from "@/state/atoms";

/// The header's disc: the whole run's activity. It turns while any session
/// is working and rests when every session is idle or none is alive.
export function WorkRoundel({ size = 16 }: { size?: number }) {
  const stages = useAtomValue(snapshotAtom)?.status.stages ?? [];
  const working = stages.filter(
    (stage) => stage.status === "executing" && stage.activity_status === "Working",
  );
  const live = stages.some((stage) => stage.session_alive);
  const title =
    working.length > 0
      ? `${working.length} session${working.length === 1 ? "" : "s"} working`
      : live
        ? "sessions idle"
        : "no live session";
  return (
    <span className="inline-flex items-center" title={title}>
      <BusyRoundel
        busy={working.length > 0}
        size={size}
        busyLabel={title}
        idleLabel={title}
        className="shrink-0"
      />
    </span>
  );
}

/// The kit's busy disc as the activity instrument for a stage with a live
/// session: it turns while the session is working and rests upright when the
/// session is idle. Absent for stages with no session to watch.
export function ActivityRoundel({ stage, size = 12 }: { stage: StageSummary; size?: number }) {
  const watched =
    stage.status === "executing" ||
    stage.status === "waiting-for-input" ||
    stage.status === "needs-handoff";
  if (!watched) return null;
  return (
    <BusyRoundel
      busy={stage.activity_status === "Working"}
      size={size}
      busyLabel="session working"
      idleLabel="session idle"
      className="shrink-0"
    />
  );
}
