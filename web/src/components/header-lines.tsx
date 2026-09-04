import { cn } from "cn";

import type { Snapshot, StageStatus } from "@/api/schema";
import { Progress } from "@/components/ui/progress";
import { StateGlyph, toneClass } from "@/components/state-badge";
import { daemonLine, progressPercent, summaryCounts } from "@/lib/format";

/// "● daemon running · tick 4s ago", toned by `daemonLine`.
export function DaemonLine({ snapshot }: { snapshot: Snapshot }) {
  const line = daemonLine(snapshot.daemon, snapshot.tick_age_secs);
  return (
    <span className={cn("inline-flex items-center gap-1.5 text-xs", toneClass(line.tone))}>
      <span aria-hidden="true" className="size-2 rounded-full bg-(--tone)" />
      <span>{line.text}</span>
      {line.detail && <span className="text-muted-foreground">· {line.detail}</span>}
    </span>
  );
}

/// "1 of 7 stages complete" with the bar and percent.
export function ProgressLine({ snapshot }: { snapshot: Snapshot }) {
  const { completed, total } = snapshot.status.progress;
  const percent = progressPercent(completed, total);
  return (
    <div className="flex items-center gap-3 text-sm">
      <span className="tabular-nums">
        {completed} of {total} stages complete
      </span>
      <Progress
        value={percent}
        aria-label={`${percent}% of stages complete`}
        className="h-1.5 w-40 max-w-[40vw] bg-muted [&>[data-slot=progress-indicator]]:bg-(--tone-completed)"
      />
      <span className="text-xs tabular-nums text-muted-foreground">{percent}%</span>
    </div>
  );
}

const COUNTS: { key: "executing" | "queued" | "waiting" | "done"; status: StageStatus }[] = [
  { key: "executing", status: "executing" },
  { key: "queued", status: "queued" },
  { key: "waiting", status: "waiting-for-deps" },
  { key: "done", status: "completed" },
];

/// "● 1 executing · ▶ 1 queued · ○ 2 waiting · 3 need attention · ✓ 1 done".
export function SummaryLine({ snapshot, attention }: { snapshot: Snapshot; attention: number }) {
  const counts = summaryCounts(snapshot.status.stages, attention);
  const [executing, queued, waiting, done] = COUNTS;
  const item = ({ key, status }: (typeof COUNTS)[number]) => (
    <span key={key} className="inline-flex items-center gap-1.5 tabular-nums">
      <StateGlyph status={status} />
      <span>{counts[key]}</span>
      <span className="text-muted-foreground">{key}</span>
    </span>
  );
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-sm">
      {item(executing)}
      <Dot />
      {item(queued)}
      <Dot />
      {item(waiting)}
      <Dot />
      <span
        className={cn(
          "tabular-nums",
          counts.attention === 0
            ? "text-muted-foreground"
            : cn(toneClass("blocked"), "font-medium"),
        )}
      >
        {counts.attention} need attention
      </span>
      <Dot />
      {item(done)}
    </div>
  );
}

/// "merged 0 · unmerged 1 · conflicts 1".
export function MergeLine({ snapshot }: { snapshot: Snapshot }) {
  const { merged, pending, conflicts } = snapshot.status.merge;
  return (
    <p className="text-xs tabular-nums text-muted-foreground">
      merged {merged.length} · unmerged {pending.length} ·{" "}
      <span className={cn(conflicts.length > 0 && toneClass("warning"))}>
        conflicts {conflicts.length}
      </span>
    </p>
  );
}

function Dot() {
  return (
    <span aria-hidden="true" className="text-muted-foreground/60">
      ·
    </span>
  );
}
