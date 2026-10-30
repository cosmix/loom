import { useAtomValue } from "jotai/react";
import { ChevronDown, ChevronUp, CircleCheck } from "lucide-react";
import { useState, type JSX, type ReactNode, type Ref } from "react";

import { useNow } from "@/components/hooks/use-now";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { ChangedSinceControl } from "@/plan/review/changed-since";
import { RunStrip } from "@/plan/run/run-strip";
import { summaryQuestionAtom, useSummaryFeed, type PanelLoad } from "@/plan/summaries/state";
import {
  QuestionAnnouncement,
  SummaryStatusRow,
  useQuestionPopover,
} from "@/plan/summaries/summary-status";
import {
  ACTIVITY_LABELS,
  ACTIVITY_TONES,
  ActivityGlyph,
  ActivityTile,
} from "@/plan/ui/activity-glyphs";

import "./activity.css";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";
import { mostUrgent, useActivities, type Activity } from "./use-activities";

/** `--plan-shadow` is defined in plan.css for both themes; the second layer adds lift. */
const POPOVER_SHADOW = "var(--plan-shadow), 0 18px 50px oklch(0 0 0 / 0.18)";

/** What `useQuestionPopover` returns; a sample pill supplies only the open state. */
interface PopoverControl {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  contentRef?: Ref<HTMLDivElement>;
  onOpenAutoFocus?: (event: Event) => void;
  onCloseAutoFocus?: (event: Event) => void;
}

/** `m:ss` under an hour, whole minutes (`65m`) from then on. */
function elapsedLabel(since: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - since) / 1000));
  if (seconds >= 3600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function ActivityList({ activities }: { activities: Activity[] }) {
  const now = useNow();

  return (
    <ul aria-label="In progress" className="activity-list">
      {activities.map((activity) => (
        <li key={activity.id} className="activity-row">
          <ActivityTile kind={activity.kind} />
          <div className="activity-row__body">
            <p className="activity-row__title" title={activity.title}>
              {activity.title}
            </p>
            {activity.subject !== null && (
              <p className="activity-row__sub" title={visibleControls(activity.subject)}>
                <Untrusted text={activity.subject} />
              </p>
            )}
          </div>
          {activity.since !== null && (
            <span className="activity-row__time">{elapsedLabel(activity.since, now)}</span>
          )}
        </li>
      ))}
    </ul>
  );
}

function PillBody({
  activities,
  compact,
  open,
}: {
  activities: Activity[];
  compact: boolean;
  open: boolean;
}): JSX.Element {
  const urgent = mostUrgent(activities);
  const Chevron = open ? ChevronUp : ChevronDown;
  return (
    <>
      {urgent ? (
        <>
          <ActivityGlyph kind={urgent.kind} />
          <span className="tabular-nums">
            {compact ? activities.length : `${activities.length} in progress`}
          </span>
        </>
      ) : (
        <>
          <CircleCheck aria-hidden="true" className="activity-pill__idle" />
          {!compact && <span>Idle</span>}
        </>
      )}
      {!compact && <Chevron aria-hidden="true" className="activity-pill__chevron" />}
    </>
  );
}

/**
 * The pill and its popover. The pill stays mounted when nothing runs, because the popover also
 * holds the run, review and summary rows. Those rows (and the run row's "daemon stopped" text)
 * exist only while the popover is open.
 */
function Pill({
  compact,
  control,
  footer,
}: {
  compact: boolean;
  control: PopoverControl;
  footer: ReactNode;
}): JSX.Element {
  const activities = useActivities();
  const urgent = mostUrgent(activities);
  const label = urgent
    ? `${activities.length} in progress: ${ACTIVITY_LABELS[urgent.kind]}`
    : "Activity: idle";

  return (
    <Popover open={control.open} onOpenChange={control.onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="activity-pill"
          data-tone={urgent ? ACTIVITY_TONES[urgent.kind] : undefined}
          aria-label={label}
        >
          <PillBody activities={activities} compact={compact} open={control.open} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        ref={control.contentRef}
        aria-label="In progress"
        align="end"
        sideOffset={8}
        collisionPadding={16}
        className="max-h-[80vh] w-[min(360px,86vw)] max-w-none gap-0 overflow-y-auto rounded-[12px] border border-border bg-card p-1.5 font-sans ring-0"
        style={{ boxShadow: POPOVER_SHADOW }}
        onOpenAutoFocus={control.onOpenAutoFocus}
        onCloseAutoFocus={control.onCloseAutoFocus}
      >
        {activities.length > 0 ? (
          <ActivityList activities={activities} />
        ) : (
          <p className="activity-empty">Nothing in progress.</p>
        )}
        {footer}
      </PopoverContent>
    </Popover>
  );
}

function Footer({ load, reload }: { load: PanelLoad; reload: () => void }): JSX.Element {
  return (
    <div className="activity-foot">
      <RunStrip />
      <ChangedSinceControl />
      <SummaryStatusRow load={load} reload={reload} />
    </div>
  );
}

/**
 * Owns the summary feed: the pill reads rebuild progress while the popover is closed, so the feed
 * cannot live in the popover's footer. A rebuild question opens the popover without taking focus.
 */
function LivePill({ compact }: { compact: boolean }): JSX.Element {
  const { load, reload } = useSummaryFeed();
  const question = useAtomValue(summaryQuestionAtom);
  const popover = useQuestionPopover(question);
  return (
    <>
      <Pill compact={compact} control={popover} footer={<Footer load={load} reload={reload} />} />
      <QuestionAnnouncement question={question} />
    </>
  );
}

function SamplePill({ compact }: { compact: boolean }): JSX.Element {
  const [open, setOpen] = useState(false);
  return <Pill compact={compact} control={{ open, onOpenChange: setOpen }} footer={null} />;
}

/**
 * `compact` shows the count only. `sample` draws the pill over seeded atoms (Help scenes) and omits
 * the footer, whose rows fetch and subscribe to live state.
 */
export function ActivityPill({
  compact,
  sample = false,
}: {
  compact: boolean;
  sample?: boolean;
}): JSX.Element {
  return sample ? <SamplePill compact={compact} /> : <LivePill compact={compact} />;
}
