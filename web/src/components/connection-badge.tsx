import { cn } from "cn";
import { useAtomValue } from "jotai/react";

import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { useNow } from "@/components/hooks/use-now";
import { toneClass } from "@/components/state-badge";
import { formatElapsed, type Tone } from "@/lib/format";
import { connectionAtom, snapshotAtom } from "@/state/atoms";

type Phase = "connecting" | "live" | "reconnecting" | "offline" | "error";

const PHASE: Record<Phase, { tone: Tone; label: string }> = {
  connecting: { tone: "dimmed", label: "connecting" },
  live: { tone: "completed", label: "live" },
  reconnecting: { tone: "warning", label: "reconnecting" },
  offline: { tone: "blocked", label: "offline" },
  error: { tone: "blocked", label: "error" },
};

/// Socket phase as a dot and a word; hover for the message, the age of the
/// last frame, and — when the server left the daemon lane — why (`notice`).
export function ConnectionBadge({ className }: { className?: string }) {
  const connection = useAtomValue(connectionAtom);
  const snapshot = useAtomValue(snapshotAtom);
  const now = useNow();
  const { tone, label } = PHASE[connection.phase];
  const generatedAt = connection.phase === "live" ? Date.parse(snapshot?.generated_at ?? "") : NaN;
  const sinceMs = Number.isNaN(generatedAt) ? connection.since : generatedAt;
  const age = formatElapsed(Math.max(0, Math.round((now - sinceMs) / 1000)));
  const ageText = connection.phase === "live" ? `last frame ${age} ago` : `${label} for ${age}`;
  const notice = snapshot?.notice;
  const degraded = snapshot?.source === "files";

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={`connection ${label}; ${ageText}`}
          className={cn(
            "inline-flex h-7 items-center gap-2 rounded-full border border-hairline bg-card px-2.5 text-xs font-medium",
            toneClass(tone),
            className,
          )}
        >
          <span
            aria-hidden="true"
            className={cn(
              "size-2 rounded-full bg-(--tone)",
              connection.phase === "live" && "dot-live",
            )}
          />
          <span className="text-foreground">{label}</span>
          {degraded && <span className={toneClass("warning")}>files</span>}
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom" align="end" className="max-w-xs flex-col items-start gap-0.5">
        <p>
          {label} · {ageText}
        </p>
        {connection.message && <p className="opacity-80">{connection.message}</p>}
        {snapshot && <p className="opacity-80">source: {snapshot.source}</p>}
        {notice && <p className="mt-1 font-mono text-[11px]">{notice}</p>}
      </TooltipContent>
    </Tooltip>
  );
}
