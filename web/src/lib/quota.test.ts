import { describe, expect, it } from "vitest";

import type { ProviderQuota, QuotaSnapshot } from "@/api/schema";
import {
  ageText,
  displayPercent,
  formatReset,
  providerRows,
  quotaHealth,
  resetText,
  windowLabel,
  windowOf,
  type QuotaHealth,
} from "@/lib/quota";

const claudeQuota: ProviderQuota = {
  observed_at: 1788523200,
  windows: [
    { kind: "five-hour", used_percent: 48, resets_at: 1788531180 },
    { kind: "seven-day", used_percent: 31, resets_at: 1788876000 },
  ],
  plan: null,
  error: null,
};

const codexQuota: ProviderQuota = {
  observed_at: 1788522960,
  windows: [{ kind: "seven-day", used_percent: 63, resets_at: 1788728400 }],
  plan: "pro",
  error: null,
};

describe("quota helpers", () => {
  it.each<readonly [number, QuotaHealth]>([
    [59.9, "green"],
    [60, "yellow"],
    [89.9, "yellow"],
    [90, "red"],
  ])("bands %s%% as %s", (percent, health) => {
    expect(quotaHealth(percent)).toBe(health);
  });

  it.each([
    ["five-hour", "5h"],
    ["seven-day", "7d"],
  ] as const)("labels %s as %s", (kind, label) => {
    expect(windowLabel(kind)).toBe(label);
  });

  it.each([
    [45, "45s"],
    [133, "2m13s"],
    [7980, "2h13m"],
    [86400, "1d0h"],
    [352800, "4d2h"],
    [-5, "0s"],
  ])("formats %i seconds until reset as %s", (seconds, expected) => {
    expect(formatReset(seconds)).toBe(expected);
  });

  it("has no reset when resetsAt is null", () => {
    expect(resetText(null, 1788523200)).toBeNull();
  });

  it("shows now for a reset that has already passed", () => {
    expect(resetText(1788523100, 1788523200)).toBe("now");
  });

  it("formats a future reset relative to now", () => {
    expect(resetText(1788531180, 1788523200)).toBe("2h13m");
  });

  it.each([
    [0, null],
    [599, null],
    [600, "10m old"],
    [90000, "1d1h old"],
  ])("shows the reading age for %i seconds as %s", (age, expected) => {
    const nowSecs = 1788523200;
    expect(ageText(nowSecs - age, nowSecs)).toBe(expected);
  });

  it("orders the providers that have data", () => {
    const snapshot: QuotaSnapshot = { claude: claudeQuota, codex: codexQuota };
    expect(providerRows(snapshot)).toEqual([
      ["claude", claudeQuota],
      ["codex", codexQuota],
    ]);
  });

  it("drops a provider with no data", () => {
    const snapshot: QuotaSnapshot = { claude: claudeQuota, codex: null };
    expect(providerRows(snapshot)).toEqual([["claude", claudeQuota]]);
  });

  it("returns no rows when neither provider has data", () => {
    const snapshot: QuotaSnapshot = { claude: null, codex: null };
    expect(providerRows(snapshot)).toEqual([]);
  });

  it("finds the window of the requested kind", () => {
    expect(windowOf(claudeQuota, "five-hour")).toEqual({
      kind: "five-hour",
      used_percent: 48,
      resets_at: 1788531180,
    });
  });

  it("returns null when the provider has no window of that kind", () => {
    expect(windowOf(codexQuota, "five-hour")).toBeNull();
  });

  it.each([
    [48.4, 48],
    [100.7, 100],
    [-1, 0],
  ])("rounds and clamps %s%% for display as %i", (percent, expected) => {
    expect(displayPercent(percent)).toBe(expected);
  });
});
