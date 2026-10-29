import { Link } from "react-router";

import { useNow } from "@/components/hooks/use-now";
import { formatElapsed } from "@/lib/format";
import type { CodexStatus } from "@/plan/api/schema-pressure";
import { toAddress } from "@/plan/files/address";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { Untrusted } from "../visible-controls";

function RunningCodex({ startedMs }: { startedMs?: number }) {
  const now = useNow();
  const seconds = startedMs === undefined ? 0 : Math.max(0, Math.floor((now - startedMs) / 1000));
  return (
    <>
      <ActivityGlyph kind="codex" />
      <span role="status" aria-live="polite" className="sr-only">
        Codex is reviewing
      </span>
      <span>Running · {formatElapsed(seconds)}</span>
    </>
  );
}

export function CodexStatusItem({
  status,
  startedMs,
}: {
  status: CodexStatus;
  startedMs?: number;
}) {
  return (
    <div className="space-y-2 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span>
          <Untrusted text={status.model} />
        </span>
        {status.state === "running" && <RunningCodex startedMs={startedMs} />}
        {status.state === "review_written" &&
          (status.report_path !== null ? (
            <Link className="underline" to={toAddress(status.report_path)}>
              Review written
            </Link>
          ) : (
            <span>Review written</span>
          ))}
        {status.state === "no_review" && <span>No review</span>}
        {status.state === "failed" && <span>Codex failed</span>}
      </div>
      {status.state === "failed" && status.log_tail !== null && (
        <pre className="whitespace-pre-wrap break-words font-mono text-xs">
          <code>
            <Untrusted text={status.log_tail} />
          </code>
        </pre>
      )}
    </div>
  );
}
