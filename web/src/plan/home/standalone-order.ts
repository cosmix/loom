import type { StandalonePlanView } from "@/plan/api/schema";
import { relativeTime } from "@/plan/files/heading-parts";

export type PlanState = StandalonePlanView["state"];
export type SortKey = "title" | "state" | "modified";
export type SortDirection = "ascending" | "descending";
export interface Sort {
  key: SortKey;
  direction: SortDirection;
}

/** Newest first until the reader picks another order. */
export const DEFAULT_SORT: Sort = { key: "modified", direction: "descending" };

// Work in progress first, then what is ready, then what is finished or set aside.
export const STATE_ORDER: Record<PlanState, number> = {
  running: 0,
  written: 1,
  done: 2,
  retired: 3,
};
export const STATE_TONE: Record<PlanState, string | undefined> = {
  running: "tone-executing",
  written: undefined,
  done: "tone-completed",
  retired: "tone-dimmed",
};

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
const PLAN_PREFIX = /^plan\s*:\s*/i;

/** A title without the "Plan:" most plan headings open with, unless nothing else is left. */
export function shortTitle(title: string): string {
  const short = title.replace(PLAN_PREFIX, "");
  return short.trim() === "" ? title : short;
}

/** The direction a header sorts in on its first click: dates newest first, words A to Z. */
export function firstDirection(key: SortKey): SortDirection {
  return key === "modified" ? "descending" : "ascending";
}

function modifiedTime(plan: StandalonePlanView): number | null {
  const time = plan.last_change === null ? Number.NaN : Date.parse(plan.last_change);
  return Number.isFinite(time) ? time : null;
}

function compareModified(a: StandalonePlanView, b: StandalonePlanView, sign: number): number {
  const [left, right] = [modifiedTime(a), modifiedTime(b)];
  // A plan whose time could not be read sorts last in either direction.
  if (left === null || right === null) return (left === null ? 1 : 0) - (right === null ? 1 : 0);
  return sign * (left - right);
}

export interface StateCount {
  state: PlanState;
  count: number;
}

/** How many plans are in each state that occurs, in STATE_ORDER (running first). */
export function stateCounts(plans: readonly StandalonePlanView[]): StateCount[] {
  const counts = new Map<PlanState, number>();
  for (const plan of plans) counts.set(plan.state, (counts.get(plan.state) ?? 0) + 1);
  return [...counts]
    .map(([state, count]) => ({ state, count }))
    .sort((a, b) => STATE_ORDER[a.state] - STATE_ORDER[b.state]);
}

/** The picked states with `state` added or removed; an empty set means every state. */
export function toggleState(
  picked: ReadonlySet<PlanState>,
  state: PlanState,
): ReadonlySet<PlanState> {
  const next = new Set(picked);
  if (next.has(state)) next.delete(state);
  else next.add(state);
  return next;
}

/**
 * The picked states that still occur in the list. A state the list no longer holds drops out, so
 * a filter never hides every plan behind a state with no button; nothing left means every state.
 */
export function activeStates(
  picked: ReadonlySet<PlanState>,
  counts: readonly StateCount[],
): ReadonlySet<PlanState> {
  return new Set(counts.map(({ state }) => state).filter((state) => picked.has(state)));
}

function matchesQuery(plan: StandalonePlanView, needle: string): boolean {
  return plan.title.toLowerCase().includes(needle) || plan.slug.toLowerCase().includes(needle);
}

/**
 * The plans the table shows: those in the picked states (all of them when none is picked) that
 * match the query, in the sort's order.
 */
export function shownPlans(
  plans: readonly StandalonePlanView[],
  query: string,
  sort: Sort,
  states: ReadonlySet<PlanState> = new Set(),
): StandalonePlanView[] {
  const needle = query.trim().toLowerCase();
  const matching = plans.filter(
    (plan) => (states.size === 0 || states.has(plan.state)) && matchesQuery(plan, needle),
  );
  const sign = sort.direction === "ascending" ? 1 : -1;
  return matching.sort((a, b) => {
    const byState = STATE_ORDER[a.state] - STATE_ORDER[b.state];
    const byTitle = collator.compare(shortTitle(a.title), shortTitle(b.title));
    if (sort.key === "modified") return compareModified(a, b, sign) || byTitle;
    return sign * (sort.key === "state" ? byState || byTitle : byTitle || byState);
  });
}

const WEEK_MS = 7 * 24 * 3600 * 1000;
const DAY_MONTH = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const DAY_MONTH_YEAR = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });
const FULL = new Intl.DateTimeFormat(undefined, { dateStyle: "long", timeStyle: "short" });

/**
 * The Modified cell's words and its tooltip: within a week "3 h ago", older "12 Oct" (with the
 * year when it is not this year's), and the full date and time as the tooltip. Null when the
 * server sent no readable time.
 */
export function modifiedLabel(
  stamp: string | null,
  now: number,
): { text: string; title: string } | null {
  if (stamp === null) return null;
  const time = Date.parse(stamp);
  if (!Number.isFinite(time)) return null;
  const date = new Date(time);
  const sameYear = date.getFullYear() === new Date(now).getFullYear();
  const text =
    now - time < WEEK_MS
      ? relativeTime(stamp, now)
      : (sameYear ? DAY_MONTH : DAY_MONTH_YEAR).format(date);
  return { text, title: FULL.format(date) };
}
