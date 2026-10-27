import { cn } from "cn";
import { ChevronRight, ChevronUp, History } from "lucide-react";
import { useState, type Ref } from "react";

import type { Attention, StageSummary } from "@/api/schema";
import {
  RECENT_EVENTS,
  historyCounts,
  historyEvents,
  type HistoryEvent,
  type HistoryMark,
} from "@/components/stage-history-model";
import { toneClass } from "@/components/state-badge";
import { LinesBox } from "@/components/text-box";
import { Button } from "@/components/ui/button";
import { formatClock, formatDayUnlessToday, formatStamp, type Tone } from "@/lib/format";
import { STAGE_STATES } from "@/lib/states";

/// Every issue the stage has had, newest first on one spine: the live one
/// marked current, the archived ones with how they ended. Shows the newest
/// few until asked for all. Renders nothing for a stage with no issues.
export function HistoryCard({
  stage,
  entry,
  ref,
}: {
  stage: StageSummary;
  entry: Attention | undefined;
  ref?: Ref<HTMLElement>;
}) {
  const [all, setAll] = useState(false);
  const events = historyEvents(stage, entry);
  if (events.length === 0) return null;
  const shown = all ? events : events.slice(0, RECENT_EVENTS);
  const noun = events.length === 1 ? "event" : "events";
  return (
    <section ref={ref} aria-label="history" className="ledger-card scroll-mt-4">
      <header className="ledger-head">
        <span className="ledger-icon">
          <History aria-hidden="true" className="size-3" />
        </span>
        <h2 className="eyebrow">history</h2>
        <span className="ml-auto text-xs text-muted-foreground">
          {events.length} {noun} · newest first
        </span>
      </header>
      <ol className="history-list">
        {shown.map((event, index) => (
          <HistoryItem key={event.key} event={event} last={index === shown.length - 1} />
        ))}
      </ol>
      <footer className="history-foot">
        <span className="text-xs text-muted-foreground">{historyCounts(events)}</span>
        {events.length > RECENT_EVENTS && (
          <Button
            type="button"
            variant="outline"
            size="sm"
            aria-expanded={all}
            onClick={() => setAll((open) => !open)}
          >
            {all ? <ChevronUp aria-hidden="true" /> : <ChevronRight aria-hidden="true" />}
            {all ? "Collapse to recent" : `Show all ${events.length} events`}
          </Button>
        )}
      </footer>
    </section>
  );
}

function HistoryItem({ event, last }: { event: HistoryEvent; last: boolean }) {
  const Icon = STAGE_STATES[event.status].icon;
  const loud = event.mark === "current" || event.evidence.length > 0;
  return (
    <li
      className={cn("history-item text-foreground", toneClass(event.tone))}
      data-mark={event.mark}
      data-loud={loud ? "" : undefined}
    >
      <span className="history-time">{event.at === null ? "now" : <Moment at={event.at} />}</span>
      <span aria-hidden="true" className="history-spine">
        <span className="history-dot" />
        {!last && <span className="history-line" />}
      </span>
      <div className="history-body">
        <div className="history-head">
          <span className="history-tag">
            <Icon aria-hidden="true" className="size-3" />
            {event.tag}
          </span>
          <span className="history-title">{event.title}</span>
          <Mark mark={event.mark} />
        </div>
        <EventFacts event={event} />
        {event.evidence.length > 0 && <EventEvidence event={event} />}
      </div>
    </li>
  );
}

const MARK_TONE: Record<HistoryMark, Tone | null> = {
  current: null,
  resolved: "completed",
  accepted: "completed",
  ruled: "completed",
  closed: "pending",
  superseded: "pending",
  reset: "pending",
  retried: "pending",
  skipped: "pending",
  "needs-evidence": "warning",
  rejected: "blocked",
};

function Mark({ mark }: { mark: HistoryMark }) {
  const tone = MARK_TONE[mark];
  return (
    <span className={cn("history-mark", tone && toneClass(tone))}>{mark.replaceAll("-", " ")}</span>
  );
}

/// A time on the report: the clock, after the day when it is not today's; the
/// full stamp on hover.
export function Moment({ at }: { at: string }) {
  const day = formatDayUnlessToday(at);
  return (
    <time dateTime={at} title={formatStamp(at)}>
      {day !== null && <span className="moment-day">{day}</span>}
      {formatClock(at)}
    </time>
  );
}

/// When an archived issue was detected and what ended it, `ended_by` verbatim.
function EventFacts({ event }: { event: HistoryEvent }) {
  if (event.endedBy === null) return null;
  return (
    <p className="text-[13px] wrap-anywhere text-muted-foreground">
      {event.detectedAt !== null && (
        <>
          detected <Moment at={event.detectedAt} />
          {" · "}
        </>
      )}
      ended: {event.endedBy}
    </p>
  );
}

function EventEvidence({ event }: { event: HistoryEvent }) {
  const count = event.evidence.length;
  return (
    <div className="flex flex-col gap-1.5">
      <span className="eyebrow">
        evidence · {count} {count === 1 ? "line" : "lines"}
      </span>
      <LinesBox label={`${event.title} evidence`} lines={event.evidence} />
    </div>
  );
}
