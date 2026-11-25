import { describe, expect, it } from "vitest";

import { clockHm } from "./plan-math";
import { sampleRecord } from "./sample-record";
import { LEFT, ROW, TOP, WIDTH, timelineLayout } from "./timeline-layout";

// One hour of wall time spans LEFT (210) to WIDTH - 70 (890): 680 units, 11.33 per minute.
const PER_MINUTE = 680 / 60;
const layout = timelineLayout(sampleRecord);
const row = (id: string) => layout.rows.find((entry) => entry.id === id)!;

describe("timelineLayout", () => {
  it("draws one row per stage at a fixed pitch", () => {
    expect(layout.rows.map((entry) => entry.id)).toEqual(["server", "docs", "web"]);
    expect(layout.rows.map((entry) => entry.y)).toEqual([TOP, TOP + ROW, TOP + 2 * ROW]);
    expect(layout.height).toBe(TOP + 3 * ROW + 18);
  });

  it("puts every bar on the one plan scale", () => {
    const server = row("server");
    expect(server.barX).toBeCloseTo(LEFT);
    expect(server.barWidth).toBeCloseTo(40 * PER_MINUTE);
    const web = row("web");
    expect(web.barX).toBeCloseTo(LEFT + 30 * PER_MINUTE);
    expect(web.barX + web.barWidth).toBeCloseTo(WIDTH - 70);
  });

  it("lays phase segments end to end from their own timestamps", () => {
    const [contract, executing] = row("server").segments;
    expect(contract!.phase).toBe("contract");
    expect(contract!.x).toBeCloseTo(LEFT);
    expect(contract!.width).toBeCloseTo(10 * PER_MINUTE);
    expect(executing!.phase).toBe("executing");
    expect(executing!.x).toBeCloseTo(LEFT + 10 * PER_MINUTE);
    expect(executing!.width).toBeCloseTo(30 * PER_MINUTE);
  });

  it("closes an open span at the stage's end and titles each segment", () => {
    const retry = row("web").segments.find((segment) => segment.phase === "retry")!;
    expect(retry.x + retry.width).toBeCloseTo(WIDTH - 70);
    expect(retry.title).toBe(
      `Retry ${clockHm("2026-11-21T09:50:00Z")}–${clockHm("2026-11-21T10:00:00Z")} (10m)`,
    );
  });

  it("draws a skipped stage as a stub with no segments", () => {
    const docs = row("docs");
    expect(docs.skipped).toBe(true);
    expect(docs.sub).toBe("skipped");
    expect(docs.segments).toEqual([]);
    expect(docs.barWidth).toBe(0);
    expect(docs.duration).toBe("");
  });

  it("labels each bar with its duration and the stage's model", () => {
    expect(row("server").duration).toBe("40m");
    expect(row("server").sub).toBe("opus/high");
    expect(row("web").sub).toBe("gpt-5.6-terra");
  });

  it("ticks every quarter hour and ends on the plan's end", () => {
    expect(layout.ticks.map((tick) => tick.x)).toEqual([LEFT, LEFT + 170, LEFT + 340, LEFT + 510]);
    expect(layout.ticks[0]!.label).toBe(clockHm(sampleRecord.started_at));
    expect(layout.end.x).toBeCloseTo(WIDTH - 70);
    expect(layout.end.label).toBe(clockHm(sampleRecord.ended_at));
  });

  it("cuts an id longer than the label column and keeps the whole id on the row", () => {
    const id = "integration-verify-and-knowledge-distill";
    const result = timelineLayout({
      ...sampleRecord,
      stages: [{ ...sampleRecord.stages[0]!, id }],
    });
    expect(result.rows[0]!.id).toBe(id);
    expect(result.rows[0]!.label).toBe("integration-verify-and-kn…");
    expect(row("server").label).toBe("server");
  });

  it("repeats the hour labels under a timeline of more than twelve rows", () => {
    expect(layout.axisRepeatY).toBeNull();
    expect(layout.gridBottom).toBe(TOP + 3 * ROW + 8);
    const stages = Array.from({ length: 60 }, (_, index) => ({
      ...sampleRecord.stages[0]!,
      id: `stage-${index}`,
    }));
    const long = timelineLayout({ ...sampleRecord, stages });
    expect(long.rows).toHaveLength(60);
    expect(long.axisRepeatY).toBe(long.gridBottom + 16);
    expect(long.height).toBeGreaterThan(long.axisRepeatY!);
  });

  it("keeps a phase of a few seconds wide enough to see", () => {
    const stage = {
      ...sampleRecord.stages[0]!,
      phases: [
        {
          phase: "adjudication" as const,
          started_at: "2026-11-21T09:00:00Z",
          ended_at: "2026-11-21T09:00:02Z",
        },
      ],
    };
    const result = timelineLayout({ ...sampleRecord, stages: [stage] });
    expect(result.rows[0]!.segments[0]!.width).toBe(2);
  });

  it("draws a finished stage with no start and no phases as an empty bar, not skipped", () => {
    const stage = { ...sampleRecord.stages[0]!, started_at: null, phases: [], execution_secs: 90 };
    const untimed = timelineLayout({ ...sampleRecord, stages: [stage] }).rows[0]!;
    expect(untimed.skipped).toBe(false);
    expect(untimed.barWidth).toBe(0);
    expect(untimed.segments).toEqual([]);
    expect(untimed.duration).toBe("1m");
  });

  it("ends an unfinished stage's bar at the latest phase end whatever the phase order", () => {
    const stage = {
      ...sampleRecord.stages[0]!,
      started_at: "2026-11-21T09:00:00Z",
      completed_at: null,
      phases: [
        {
          phase: "executing" as const,
          started_at: "2026-11-21T09:00:00Z",
          ended_at: "2026-11-21T09:30:00Z",
        },
        {
          phase: "contract" as const,
          started_at: "2026-11-21T09:00:00Z",
          ended_at: "2026-11-21T09:10:00Z",
        },
      ],
    };
    const open = timelineLayout({ ...sampleRecord, stages: [stage] }).rows[0]!;
    expect(open.duration).toBe("30m");
    expect(open.barWidth).toBeCloseTo(30 * PER_MINUTE);
  });

  it("extends a completed stage's bar over a merge wait that ends after completion", () => {
    const stage = {
      ...sampleRecord.stages[0]!,
      started_at: "2026-11-21T09:00:00Z",
      completed_at: "2026-11-21T09:40:00Z",
      phases: [
        {
          phase: "executing" as const,
          started_at: "2026-11-21T09:00:00Z",
          ended_at: "2026-11-21T09:40:00Z",
        },
        {
          phase: "waiting" as const,
          started_at: "2026-11-21T09:40:01Z",
          ended_at: "2026-11-21T09:55:00Z",
        },
      ],
    };
    const held = timelineLayout({ ...sampleRecord, stages: [stage] }).rows[0]!;
    expect(held.barWidth).toBeCloseTo(55 * PER_MINUTE);
    expect(held.duration).toBe("55m");
  });
});
