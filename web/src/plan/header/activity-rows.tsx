import { useAtomValue } from "jotai/react";
import { ChevronDown, ChevronUp, TriangleAlert } from "lucide-react";
import { Popover as PopoverPrimitive } from "radix-ui";
import type { JSX, ReactNode } from "react";
import { Link } from "react-router";

import { useNow } from "@/components/hooks/use-now";
import { sessionHomeAtom } from "@/plan/chat/session-home";
import { useSessionBrief } from "@/plan/chat/use-session-brief";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DashboardButton } from "@/plan/run/dashboard-button";
import { runStripAtom } from "@/plan/run/use-run";
import type { PanelLoad } from "@/plan/summaries/state";
import {
  SummaryDetail,
  summaryRowName,
  type SummaryExpansion,
} from "@/plan/summaries/summary-status";
import { ACTIVITY_LABELS, ActivityTile } from "@/plan/ui/activity-glyphs";
import { DOC_KIND_GLYPHS } from "@/plan/ui/glyphs";

import { stagesMerged, type Activity } from "./use-activities";

/** What only the live pill has: the summary feed and the one summary expansion state. */
export interface LiveRows {
  load: PanelLoad;
  reload: () => void;
  expansion: SummaryExpansion;
}

const PlanGlyph = DOC_KIND_GLYPHS.plan;

const HOUR_SECS = 3600;
const DAY_SECS = 24 * HOUR_SECS;

/** `m:ss` under an hour, whole minutes (`65m`) under a day, whole days (`5d`) from then on. */
function elapsedLabel(since: number, now: number): string {
  const seconds = Math.max(0, Math.floor((now - since) / 1000));
  if (seconds >= DAY_SECS) return `${Math.floor(seconds / DAY_SECS)}d`;
  if (seconds >= HOUR_SECS) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

function Elapsed({ since, now }: { since: number | null; now: number }) {
  return since === null ? null : <span>{elapsedLabel(since, now)}</span>;
}

/** Counted work (DESIGN.md, Status, rule 5): a determinate bar that carries the count. */
function ProgressBar({ done, total }: { done: number; total: number }): JSX.Element {
  return (
    <span
      role="progressbar"
      aria-label="Pressure test progress"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={done}
      className="activity-row__bar"
    >
      <span style={{ width: `${(done / total) * 100}%` }} />
    </span>
  );
}

/** The text column every row shares: a title, then one subject line that truncates. */
function RowText({
  title,
  subject,
  extra,
  progress,
}: {
  title: string;
  subject: string | null;
  extra?: ReactNode;
  progress?: Activity["progress"];
}): JSX.Element {
  return (
    <span className="activity-row__body">
      <span className="activity-row__title" title={visibleControls(title)}>
        <Untrusted text={title} />
      </span>
      {(subject !== null || extra) && (
        <span className="activity-row__sub activity-row__facts">
          {subject !== null && (
            <span className="activity-row__subject" title={visibleControls(subject)}>
              <Untrusted text={subject} />
            </span>
          )}
          {extra}
        </span>
      )}
      {progress && <ProgressBar {...progress} />}
    </span>
  );
}

/** "n need attention": the glyph takes the warning tone, the words keep the line's colour. */
function attention(count: number | undefined): ReactNode {
  if (!count || count <= 0) return undefined;
  return (
    <span className="activity-attention">
      <TriangleAlert aria-hidden="true" className="activity-attention__glyph" />
      {count} need attention
    </span>
  );
}

function RunDashboard({ url }: { url: string | null }) {
  if (!url) return null;
  return (
    <div className="activity-row__action">
      <DashboardButton variant="link" />
    </div>
  );
}

/** Where a waiting chat's row leads: a kickoff to its screen, any other chat to its home. */
function useRowHref(activity: Activity): string | null {
  const home = useAtomValue(sessionHomeAtom);
  if (!activity.session) return null;
  return activity.session.kickoff ? `/new?session=${activity.session.id}` : (home?.href ?? null);
}

function PlainRow({ activity, now }: { activity: Activity; now: number }) {
  const href = useRowHref(activity);
  const content = (
    <>
      <ActivityTile kind={activity.kind} />
      <RowText title={activity.title} subject={activity.subject} progress={activity.progress} />
      <span className="activity-row__time">
        <Elapsed since={activity.since} now={now} />
      </span>
    </>
  );
  if (href === null) return <li className="activity-row">{content}</li>;
  return (
    <li>
      {/* Following the link leaves the popover behind it, so the link closes it. */}
      <PopoverPrimitive.Close asChild>
        <Link to={href} className="activity-row">
          {content}
        </Link>
      </PopoverPrimitive.Close>
    </li>
  );
}

/**
 * A waiting chat with no name of its own (no draft name yet) takes its session brief's label as
 * its title. Only the live list draws this row: a sample pill must not fetch.
 */
function BriefTitledRow({ activity, now }: { activity: Activity; now: number }) {
  const brief = useSessionBrief(activity.session?.id ?? null);
  const titled =
    brief.status === "ready"
      ? { ...activity, title: brief.brief.label, subject: ACTIVITY_LABELS.waiting }
      : activity;
  return <PlainRow activity={titled} now={now} />;
}

/** The one place a running plan shows: its plan and merged count, attention and dashboard. */
function RunRow({ activity, now }: { activity: Activity; now: number }) {
  const strip = useAtomValue(runStripAtom);
  return (
    <li className="activity-row">
      <ActivityTile kind="running" />
      <RowText
        title={activity.title}
        subject={activity.subject}
        extra={attention(strip?.attention)}
      />
      {strip?.dashboard_url ? (
        <RunDashboard url={strip.dashboard_url} />
      ) : (
        <span className="activity-row__time">
          <Elapsed since={activity.since} now={now} />
        </span>
      )}
    </li>
  );
}

/** A running rebuild's row opens the queue, failures and any question below it. */
function SummaryRow({ activity, now, live }: { activity: Activity; now: number; live: LiveRows }) {
  const { expanded, toggle } = live.expansion;
  const Chevron = expanded ? ChevronUp : ChevronDown;
  return (
    <li>
      <button
        type="button"
        className="activity-row"
        aria-expanded={expanded}
        aria-label={summaryRowName(
          activity.subject ? `${activity.title}, ${activity.subject}` : activity.title,
        )}
        onClick={toggle}
      >
        <ActivityTile kind="summaries" />
        <RowText title={activity.title} subject={activity.subject} />
        <span className="activity-row__time">
          <Elapsed since={activity.since} now={now} />
          <Chevron aria-hidden="true" className="activity-row__chevron" />
        </span>
      </button>
      {expanded && <SummaryDetail load={live.load} reload={live.reload} />}
    </li>
  );
}

/** A sample pill (`live` absent) draws every row plain: its rows must not fetch. */
export function ActivityList({
  activities,
  live,
}: {
  activities: Activity[];
  live?: LiveRows;
}): JSX.Element {
  const now = useNow();
  return (
    <ul aria-label="In progress" className="activity-list">
      {activities.map((activity) => {
        if (live && activity.kind === "running")
          return <RunRow key={activity.id} activity={activity} now={now} />;
        if (live && activity.session && !activity.session.named)
          return <BriefTitledRow key={activity.id} activity={activity} now={now} />;
        if (live && activity.kind === "summaries")
          return <SummaryRow key={activity.id} activity={activity} now={now} live={live} />;
        return <PlainRow key={activity.id} activity={activity} now={now} />;
      })}
    </ul>
  );
}

/**
 * A plan whose daemon stopped keeps its name, counts and dashboard here, as a control: it draws
 * no running glyph and counts in no list. Absent while the run is listed, so one line shows it.
 */
export function RunLine({ listed }: { listed: boolean }) {
  const strip = useAtomValue(runStripAtom);
  if (listed || !strip?.plan) return null;
  const plan = strip.plan.split("/").at(-1) ?? strip.plan;
  return (
    <div className="activity-row">
      <ActivityTile>
        <PlanGlyph aria-hidden="true" />
      </ActivityTile>
      <RowText title={plan} subject={stagesMerged(strip)} extra={attention(strip.attention)} />
      <RunDashboard url={strip.dashboard_url} />
    </div>
  );
}
