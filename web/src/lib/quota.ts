import type { ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind } from "@/api/schema";
import { formatElapsed } from "@/lib/format";

export type QuotaHealth = "green" | "yellow" | "red";
export type ProviderName = "claude" | "codex";

export const STALE_AFTER_SECS = 600;

/** Same thresholds as `contextUsage` in format.ts: red at 90, yellow at 60, green below. */
export function quotaHealth(percent: number): QuotaHealth {
  return percent >= 90 ? "red" : percent >= 60 ? "yellow" : "green";
}

/** "5h" | "7d" */
export function windowLabel(kind: WindowKind): string {
  return kind === "five-hour" ? "5h" : "7d";
}

/**
 * Below one day delegates to formatElapsed ("45s", "2m13s", "2h13m"); at or
 * above 86400s returns "<d>d<h>h" ("4d2h"). Negative input is treated as 0.
 * Mirrors Rust `format_reset`.
 */
export function formatReset(seconds: number): string {
  const clamped = Math.max(0, seconds);
  if (clamped < 86400) {
    return formatElapsed(clamped);
  }
  const days = Math.trunc(clamped / 86400);
  const hours = Math.trunc((clamped % 86400) / 3600);
  return `${days}d${hours}h`;
}

/**
 * null when resetsAt is null; "now" when resetsAt <= nowSecs; otherwise
 * formatReset(resetsAt - nowSecs). The caller adds any prefix ("resets in
 * ..."). Mirrors Rust `reset_text`.
 */
export function resetText(resetsAt: number | null, nowSecs: number): string | null {
  if (resetsAt === null) {
    return null;
  }
  if (resetsAt <= nowSecs) {
    return "now";
  }
  return formatReset(resetsAt - nowSecs);
}

/**
 * null while the reading is younger than STALE_AFTER_SECS; otherwise the age
 * with minute granularity under a day and formatReset above it. Mirrors the
 * TUI's "· codex 4m old" suffix.
 */
export function ageText(observedAt: number, nowSecs: number): string | null {
  const age = Math.max(0, nowSecs - observedAt);
  if (age < STALE_AFTER_SECS) {
    return null;
  }
  if (age < 86400) {
    return `${Math.trunc(age / 60)}m old`;
  }
  return `${formatReset(age)} old`;
}

/** Ordered pairs for the providers that have data: [["claude", q], ["codex", q]] minus the nulls. */
export function providerRows(snapshot: QuotaSnapshot): Array<[ProviderName, ProviderQuota]> {
  const rows: Array<[ProviderName, ProviderQuota]> = [];
  if (snapshot.claude) {
    rows.push(["claude", snapshot.claude]);
  }
  if (snapshot.codex) {
    rows.push(["codex", snapshot.codex]);
  }
  return rows;
}

/** The window of that kind, or null — so a renderer can always draw both slots ("5h —"). */
export function windowOf(quota: ProviderQuota, kind: WindowKind): QuotaWindow | null {
  return quota.windows.find((window) => window.kind === kind) ?? null;
}

/** Math.round(percent) clamped to 0..100, for display. */
export function displayPercent(percent: number): number {
  return Math.max(0, Math.min(100, Math.round(percent)));
}
