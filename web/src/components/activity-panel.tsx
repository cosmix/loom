import { useAtomValue } from "jotai/react";
import { ActivityIcon } from "lucide-react";
import { Link } from "react-router";

import { EmptyState } from "@/aurora-ui/feedback/EmptyState";

import { useNow } from "@/components/hooks/use-now";
import { stageHref } from "@/components/stage-href";
import { StateGlyph } from "@/components/state-badge";
import { formatElapsed } from "@/lib/format";
import { activityLogAtom } from "@/state/atoms";

/// The stage transitions this page has seen, newest first, each with its
/// glyph and age. The first frame is a baseline, so the list starts empty.
export function ActivityPanel() {
  const log = useAtomValue(activityLogAtom);
  const now = useNow();
  const newestFirst = log.toReversed();

  return (
    <section aria-labelledby="activity-title" className="flex flex-col gap-3">
      <h2 id="activity-title" className="eyebrow">
        activity
      </h2>
      {newestFirst.length === 0 ? (
        <EmptyState
          icon={ActivityIcon}
          title="Nothing has changed since this page opened"
          description="Stage transitions land here as the daemon reports them."
          tone="muted"
          size="sm"
        />
      ) : (
        <ol className="flex flex-col divide-y divide-hairline rounded-lg border border-hairline bg-card text-sm">
          {newestFirst.map((entry) => (
            <li
              key={`${entry.at}:${entry.stageId}:${entry.status}`}
              className="flex items-center gap-3 px-3 py-1.5"
            >
              <StateGlyph status={entry.status} />
              <span className="min-w-0 flex-1 truncate">
                <Link to={stageHref(entry.stageId)} className="font-mono text-xs hover:underline">
                  {entry.stageId}
                </Link>{" "}
                <span className="text-muted-foreground">
                  {entry.message.replace(`${entry.stageId} `, "")}
                </span>
              </span>
              <time
                dateTime={new Date(entry.at).toISOString()}
                className="shrink-0 font-mono text-[11px] text-muted-foreground tabular-nums"
              >
                {formatElapsed(Math.max(0, Math.round((now - entry.at) / 1000)))} ago
              </time>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}
