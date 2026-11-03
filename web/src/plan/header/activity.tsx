import { useAtomValue } from "jotai/react";
import { ChevronDown, ChevronUp, CircleCheck } from "lucide-react";
import { useState, type JSX, type ReactNode, type Ref } from "react";

import { ChangedSinceControl } from "@/plan/review/changed-since";
import { summaryQuestionAtom, useSummaryFeed } from "@/plan/summaries/state";
import {
  QuestionAnnouncement,
  SummaryStatusRow,
  useQuestionPopover,
  useSummaryExpansion,
} from "@/plan/summaries/summary-status";
import { ACTIVITY_LABELS, ACTIVITY_TONES, ActivityGlyph } from "@/plan/ui/activity-glyphs";

import "./activity.css";
import { ActivityList, RunLine, type LiveRows } from "./activity-rows";
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
 * The pill and its popover. The pill stays mounted when nothing runs, because the popover's
 * footer also holds the stopped run, review and summary controls; those mount only while open.
 */
function Pill({
  compact,
  control,
  activities,
  live,
  footer,
}: {
  compact: boolean;
  control: PopoverControl;
  activities: Activity[];
  live?: LiveRows;
  footer: ReactNode;
}): JSX.Element {
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
        className="activity-popover max-h-[80vh] w-[min(380px,92vw)] max-w-none gap-0 overflow-x-hidden overflow-y-auto rounded-[12px] border border-border bg-card p-1 font-sans ring-0"
        style={{ boxShadow: POPOVER_SHADOW }}
        onOpenAutoFocus={control.onOpenAutoFocus}
        onCloseAutoFocus={control.onCloseAutoFocus}
      >
        {activities.length > 0 ? (
          <ActivityList activities={activities} live={live} />
        ) : (
          <p className="activity-empty">Nothing in progress.</p>
        )}
        {footer}
      </PopoverContent>
    </Popover>
  );
}

/**
 * Controls only: what is in progress is listed above, once. The run and summary controls step
 * aside while their operation is listed, so each thing has one line.
 */
function Footer({ activities, live }: { activities: Activity[]; live: LiveRows }): JSX.Element {
  const listed = (kind: Activity["kind"]) => activities.some((entry) => entry.kind === kind);
  return (
    <div className="activity-foot">
      <RunLine listed={listed("running")} />
      <ChangedSinceControl />
      {!listed("summaries") && (
        <SummaryStatusRow load={live.load} reload={live.reload} expansion={live.expansion} />
      )}
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
  const expansion = useSummaryExpansion();
  const activities = useActivities();
  const live: LiveRows = { load, reload, expansion };
  return (
    <>
      <Pill
        compact={compact}
        control={popover}
        activities={activities}
        live={live}
        footer={<Footer activities={activities} live={live} />}
      />
      <QuestionAnnouncement question={question} />
    </>
  );
}

function SamplePill({ compact }: { compact: boolean }): JSX.Element {
  const [open, setOpen] = useState(false);
  const activities = useActivities();
  return (
    <Pill
      compact={compact}
      control={{ open, onOpenChange: setOpen }}
      activities={activities}
      footer={null}
    />
  );
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
