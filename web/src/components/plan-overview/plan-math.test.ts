import { describe, expect, it } from "vitest";

import type { PlanStats } from "@/api/schema";

import {
  EMPTY,
  allTokens,
  archiveFileName,
  countsText,
  formatSpan,
  freshText,
  hitText,
  modelsByFresh,
  parallelText,
  stageCounts,
} from "./plan-math";
import { sampleRecord } from "./sample-record";
import { summaryText } from "./summary-text";

const noTokens = { input: 0, cache_write: 0, cache_read: 0, output: 0 };

describe("totals arithmetic", () => {
  it("counts fresh as input plus cache write plus output", () => {
    // 1200 + 3400 + 780 = 5380; the 56000 cache reads are not fresh.
    expect(freshText(sampleRecord)).toBe("5.3k");
    expect(allTokens(sampleRecord)).toBe(61380);
  });

  it("takes the cache hit as reads over reads, input and cache write", () => {
    // 56000 / (56000 + 1200 + 3400) = 92.4%
    expect(hitText(sampleRecord)).toBe("92.4%");
  });

  it("shows the header rail's em dash when nothing was read or written", () => {
    const empty: PlanStats = { ...sampleRecord, totals: noTokens };
    expect(hitText(empty)).toBe(EMPTY);
    const outputOnly: PlanStats = { ...sampleRecord, totals: { ...noTokens, output: 9 } };
    expect(hitText(outputOnly)).toBe(EMPTY);
  });

  it("reports the parallel factor as execution over wall time", () => {
    expect(parallelText(sampleRecord)).toBe("1.50× parallel");
    expect(parallelText({ ...sampleRecord, wall_secs: 0 })).toBeNull();
  });

  it("counts completed and skipped stages", () => {
    expect(stageCounts(sampleRecord)).toEqual({ completed: 2, skipped: 1 });
    expect(countsText(sampleRecord)).toBe("2 completed · 1 skipped");
    const none = { ...sampleRecord, stages: sampleRecord.stages.slice(0, 1) };
    expect(countsText(none)).toBe("1 completed");
  });
});

describe("formatSpan", () => {
  it.each([
    [0, "0s"],
    [45, "45s"],
    [60, "1m"],
    [2700, "45m"],
    [3600, "1h 00m"],
    [23700, "6h 35m"],
  ])("%d seconds reads %s", (secs, text) => {
    expect(formatSpan(secs)).toBe(text);
  });
});

describe("models and archive", () => {
  it("orders models by fresh tokens, largest first", () => {
    expect(modelsByFresh(sampleRecord).map((row) => row.model)).toEqual(["opus", "gpt-5.6-terra"]);
  });

  it("names the archive after the plan id", () => {
    expect(archiveFileName(sampleRecord)).toBe(`${sampleRecord.plan_id}.json`);
  });
});

describe("summaryText", () => {
  it("writes the totals on one line", () => {
    expect(summaryText(sampleRecord)).toBe(
      "Sample plan: 2 completed · 1 skipped, 1h 00m wall, 1h 30m execution, 5.3k fresh tokens, 92.4% cache hit, 61.3k tokens total",
    );
  });
});
