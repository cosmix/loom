import { Check, Minus, TriangleAlert } from "lucide-react";
import type { JSX } from "react";
import { Link } from "react-router";

import type { CodexState, CodexStatus } from "@/plan/api/schema-pressure";
import { toAddress } from "@/plan/files/address";
import { ActivityTile } from "@/plan/ui/activity-glyphs";
import { Untrusted } from "../visible-controls";
import { LiveElapsed } from "./elapsed";

const SETTLED_ICON = {
  review_written: <Check />,
  no_review: <Minus />,
  failed: <TriangleAlert />,
} as const satisfies Record<Exclude<CodexState, "running">, JSX.Element>;

function CodexTile({ state }: { state: CodexState }): JSX.Element {
  // Only a running review animates: a settled one shows a static mark.
  return state === "running" ? (
    <ActivityTile kind="codex" />
  ) : (
    <ActivityTile>{SETTLED_ICON[state]}</ActivityTile>
  );
}

function CodexOutcome({ status }: { status: CodexStatus }): JSX.Element {
  switch (status.state) {
    case "running":
      return <span>reviewing the plan in the background</span>;
    case "review_written":
      return status.report_path !== null ? (
        <Link className="underline" to={toAddress(status.report_path)}>
          Review written
        </Link>
      ) : (
        <span>Review written</span>
      );
    case "no_review":
      return <span>No review</span>;
    case "failed":
      return <span>Codex failed</span>;
  }
}

/** The Codex card of a round: the model, what Codex is doing, and a mono clock while it runs. */
export function CodexStatusItem({
  status,
  startedMs,
}: {
  status: CodexStatus;
  startedMs?: number;
}): JSX.Element {
  const running = status.state === "running";
  return (
    <div className="grid grid-cols-[26px_minmax(0,1fr)_auto] items-center gap-x-2.5 gap-y-2 rounded-[10px] border border-hairline bg-background px-3 py-2.5 text-sm">
      <CodexTile state={status.state} />
      <div className="min-w-0 [overflow-wrap:anywhere]">
        <p className="text-[13px] font-semibold">
          Codex <code className="font-mono text-xs font-normal">$pressure</code>
        </p>
        <p className="text-xs text-muted-foreground">
          <Untrusted text={status.model} /> · <CodexOutcome status={status} />
        </p>
        {running && (
          <span role="status" aria-live="polite" className="sr-only">
            Codex is reviewing
          </span>
        )}
      </div>
      {running && (
        <span className="font-mono text-[11.5px] text-muted-foreground">
          <LiveElapsed startMs={startedMs} />
        </span>
      )}
      {status.state === "failed" && status.log_tail !== null && (
        <pre className="col-span-3 font-mono text-xs break-words whitespace-pre-wrap">
          <code>
            <Untrusted text={status.log_tail} />
          </code>
        </pre>
      )}
    </div>
  );
}
