import { useAtomValue } from "jotai";

import { DashboardButton } from "./dashboard-button";
import { runStripAtom } from "./use-run";

export function RunStrip({ compact = false }: { compact?: boolean }) {
  const strip = useAtomValue(runStripAtom);
  if (!strip?.plan) return null;
  const text = `${strip.plan} · daemon ${strip.daemon_running ? "running" : "stopped"} · ${strip.merged}/${strip.total} merged · ${strip.attention} need attention`;
  return (
    <div className="flex items-center gap-2 text-sm">
      {compact ? (
        <span aria-label={text} role="status">
          <span aria-hidden="true">
            {strip.daemon_running ? "●" : "○"} {strip.attention}
          </span>
        </span>
      ) : (
        <span>{text}</span>
      )}
      <DashboardButton />
    </div>
  );
}
