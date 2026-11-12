import { useAtomValue } from "jotai/react";
import { ChevronDown, ChevronUp, CircleCheck } from "lucide-react";
import { useState, type JSX, type ReactNode, type Ref } from "react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import { ChangedSinceControl } from "@/plan/review/changed-since";
import { panelLoadAtom, summaryQuestionAtom, summaryReloadAtom } from "@/plan/summaries/state";
import {
  QuestionAnnouncement,
  SummaryStatusRow,
  useQuestionPopover,
  useSummaryExpansion,
} from "@/plan/summaries/summary-status";
import {
  ACTIVITY_LABELS,
  ACTIVITY_TONES,
  ActivityGlyph,
  isProgressKind,
  type ActivityKind,
} from "@/plan/ui/activity-glyphs";
import { LoadState } from "@/plan/ui/load-state";

import "./activity.css";
import { ActivityList, RunLine, type LiveRows } from "./activity-rows";
import { Popover, PopoverContent, PopoverTrigger } from "./popover";
import { inProgress, mostUrgent, useActivities, type Activity } from "./use-activities";

/**
 * The floating shadow every popover carries (`PopoverContent`): the halo and near drop over the
 * longer drop, both tokens in plan.css.
 */
const POPOVER_SHADOW = "var(--plan-shadow), var(--popover-lift)";

/** What `useQuestionPopover` returns; a sample pill supplies only the open state. */
interface PopoverControl {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  contentRef?: Ref<HTMLDivElement>;
  onOpenAutoFocus?: (event: Event) => void;
  onCloseAutoFocus?: (event: Event) => void;
}

/**
 * The pill's lead glyph. Work in progress here is the roundel, the planner's one (`ui/DESIGN.md`,
 * Status); a "your turn" kind keeps its bell or LED. Decorative: the pill's words are the label.
 */
function PillGlyph({ kind }: { kind: ActivityKind }): JSX.Element {
  if (!isProgressKind(kind)) return <ActivityGlyph kind={kind} />;
  return (
    <span className="activity-glyph" data-kind={kind} aria-hidden="true">
      <BusyRoundel busy size="100%" busyLabel="" idleLabel="" />
    </span>
  );
}

/**
 * The row the pill leads with. Background tasks are no progress, so the most urgent of the rest
 * leads; the pill shows the background row alone only when nothing else is listed.
 */
function pillLead(activities: Activity[]): Activity | null {
  return mostUrgent(inProgress(activities)) ?? mostUrgent(activities);
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
  const counted = inProgress(activities);
  const urgent = pillLead(activities);
  const Chevron = open ? ChevronUp : ChevronDown;
  return (
    <>
      {urgent ? (
        <>
          <PillGlyph kind={urgent.kind} />
          {counted.length > 0 ? (
            <span className="tabular-nums">
              {compact ? counted.length : `${counted.length} in progress`}
            </span>
          ) : (
            !compact && <span>{urgent.title}</span>
          )}
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
  const urgent = pillLead(activities);
  const counted = inProgress(activities).length;
  const label = urgent
    ? counted > 0
      ? `${counted} in progress: ${ACTIVITY_LABELS[urgent.kind]}`
      : urgent.title
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
      <PillContent control={control} activities={activities} live={live} footer={footer} />
    </Popover>
  );
}

function PillContent({
  control,
  activities,
  live,
  footer,
}: {
  control: PopoverControl;
  activities: Activity[];
  live?: LiveRows;
  footer: ReactNode;
}): JSX.Element {
  return (
    <PopoverContent
      ref={control.contentRef}
      aria-label="In progress"
      align="end"
      sideOffset={8}
      collisionPadding={16}
      className="activity-popover max-h-[80vh] w-[min(380px,92vw)] max-w-none gap-0 overflow-hidden rounded-[12px] border border-border bg-card p-1 font-sans ring-0"
      style={{ boxShadow: POPOVER_SHADOW }}
      onOpenAutoFocus={control.onOpenAutoFocus}
      onCloseAutoFocus={control.onCloseAutoFocus}
    >
      {/* The list and any detail it opens scroll here; the footer below stays in view. */}
      <div className="activity-scroll">
        {activities.length > 0 ? (
          <ActivityList activities={activities} live={live} />
        ) : (
          <LoadState state="empty" size="compact" title="Nothing in progress." />
        )}
      </div>
      {footer}
    </PopoverContent>
  );
}

/**
 * Controls only: what is in progress is listed above, once. The run and summary controls step
 * aside while their operation is listed, so each thing has one line. The summary row comes first
 * and its detail scrolls under it, so the run and review lines keep the popover's bottom edge.
 */
function Footer({ activities, live }: { activities: Activity[]; live: LiveRows }): JSX.Element {
  const listed = (kind: Activity["kind"]) => activities.some((entry) => entry.kind === kind);
  return (
    <div className="activity-foot">
      {!listed("summaries") && (
        <div className="activity-foot__scroll">
          <SummaryStatusRow load={live.load} reload={live.reload} expansion={live.expansion} />
        </div>
      )}
      <RunLine listed={listed("running")} />
      <ChangedSinceControl />
    </div>
  );
}

/**
 * Reads the summary feed the shell runs (`SummaryFeedHost`): the pill needs rebuild progress while
 * the popover is closed, so it cannot be read from the popover's footer. A rebuild question opens
 * the popover without taking focus.
 */
function LivePill({ compact }: { compact: boolean }): JSX.Element {
  const load = useAtomValue(panelLoadAtom);
  const { reload } = useAtomValue(summaryReloadAtom);
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
