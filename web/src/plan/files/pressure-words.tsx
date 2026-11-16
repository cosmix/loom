import { Gauge } from "lucide-react";

import type { HeadingTone } from "@/plan/ui/doc-heading";
import { OTHER_GLYPHS, type Glyph } from "@/plan/ui/glyphs";
import { ArrowSpinner } from "@/plan/ui/spinner";

// A plan's pressure record in words, shared by the heading's status, the Pressure-test button
// (whose label is the state), the pressure record card, the spine and the file tree.

const DAY_MONTH = new Intl.DateTimeFormat(undefined, { day: "numeric", month: "short" });
const DAY_MONTH_YEAR = new Intl.DateTimeFormat(undefined, { dateStyle: "medium" });

/**
 * When a plan was pressure-tested, from its `YYYY-MM-DD` record: "today", "5 Nov" this year,
 * "5 Nov 2025" from another. A date in any other shape is shown as sent.
 */
export function pressureWhen(date: string, now: Date = new Date()): string {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date);
  if (parts === null) return date;
  const day = new Date(Number(parts[1]), Number(parts[2]) - 1, Number(parts[3]));
  if (Number.isNaN(day.getTime())) return date;
  if (day.toDateString() === now.toDateString()) return "today";
  const format = day.getFullYear() === now.getFullYear() ? DAY_MONTH : DAY_MONTH_YEAR;
  return format.format(day);
}

export const PRESSURE_TESTED = "Pressure-tested";
export const NOT_PRESSURE_TESTED = "Not pressure-tested";
/** The words of a link to a plan's pressure record, on its card and in the Pressure-test menu. */
export const READ_PRESSURE_RECORD = "Read the record";
export const pressureIcon = (date: string | null) =>
  date === null ? Gauge : OTHER_GLYPHS.pressureTested;

export interface PressureStatus {
  label: string;
  Icon: Glyph;
  tone: HeadingTone;
  /** The record's date as sent, for a tooltip beside the worded one. */
  date: string | null;
}

/**
 * "Not pressure-tested" (warn, the gauge), "Pressure-tested <when>" (ok, the flask) or "Being
 * pressure-tested" (go, the rotating arrows: a state word) while a test runs on the plan.
 */
export function pressureStatus(date: string | null, active: boolean): PressureStatus {
  if (active) {
    return { label: "Being pressure-tested", Icon: ArrowSpinner, tone: "accent", date: null };
  }
  if (date === null) {
    return { label: NOT_PRESSURE_TESTED, Icon: Gauge, tone: "warning", date: null };
  }
  return {
    label: `${PRESSURE_TESTED} ${pressureWhen(date)}`,
    Icon: OTHER_GLYPHS.pressureTested,
    tone: "ok",
    date,
  };
}
