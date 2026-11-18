import { describe, expect, it, vi } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { dynamicPart, fixture } from "@/api/frames-kit";
import { IMMUTABLE_STAGE_KEYS, parseFrame, stageDynamicSchema } from "@/api/frames";
import { stageSummarySchema } from "@/api/schema";

const server = fixture.status.stages.find((stage) => stage.id === "server")!;

describe("parseFrame", () => {
  it("parses a full frame into a snapshot", () => {
    const result = parseFrame({ t: "full", snapshot: fixtureJson });

    expect(result).toEqual({ success: true, data: { t: "full", snapshot: fixture } });
  });

  it("reports a defective snapshot with a path under snapshot", () => {
    const broken = structuredClone(fixtureJson) as { status: { stages: { status: string }[] } };
    broken.status.stages[0].status = "bogus";

    const result = parseFrame({ t: "full", snapshot: broken });

    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.issues[0].path.slice(0, 3)).toEqual(["snapshot", "status", "stages"]);
    }
  });

  it("parses a tick frame, with and without a tick age", () => {
    const tick = { t: "tick", at: "2026-09-04T12:00:01Z", stages: { server: { elapsed: 906 } } };

    expect(parseFrame({ ...tick, tick_age: 3 }).success).toBe(true);
    expect(parseFrame({ ...tick, tick_age: null }).success).toBe(true);
    expect(parseFrame({ ...tick, tick_age: 3, stages: {} }).success).toBe(true);
  });

  it("rejects a tick frame with an unknown clock key or a missing tick age", () => {
    const at = "2026-09-04T12:00:01Z";

    expect(
      parseFrame({ t: "tick", at, tick_age: 1, stages: { server: { wall: 1 } } }).success,
    ).toBe(false);
    expect(parseFrame({ t: "tick", at, stages: {} }).success).toBe(false);
  });

  it("parses a state frame with every part", () => {
    const result = parseFrame({
      t: "state",
      at: "2026-09-04T12:00:01Z",
      stages: { server: dynamicPart(server) },
      status: { plan_name: "Another plan", progress: fixture.status.progress },
      attention: fixture.attention,
      alerts: fixture.alerts,
      daemon: "running",
      source: "daemon",
      notice: null,
    });

    expect(result.success).toBe(true);
  });

  it("rejects an immutable key in a state stage", () => {
    const result = parseFrame({
      t: "state",
      at: "2026-09-04T12:00:01Z",
      stages: { server: { ...dynamicPart(server), name: "renamed" } },
    });

    expect(result.success).toBe(false);
  });

  it("rejects a status key that belongs to another message", () => {
    const base = { t: "state", at: "2026-09-04T12:00:01Z" };

    expect(parseFrame({ ...base, status: { tokens: null } }).success).toBe(false);
    expect(parseFrame({ ...base, status: { stages: [] } }).success).toBe(false);
    expect(parseFrame({ ...base, version: "1" }).success).toBe(false);
  });

  it("rejects an unknown frame kind", () => {
    expect(parseFrame({ t: "other" }).success).toBe(false);
    expect(parseFrame(fixtureJson).success).toBe(false);
  });

  it("parses a tokens frame, with null meaning no flow", () => {
    const flow = parseFrame({ t: "tokens", tokens: fixtureJson.status.tokens });
    const none = parseFrame({ t: "tokens", tokens: null });

    expect(flow).toEqual({ success: true, data: { t: "tokens", tokens: fixture.status.tokens } });
    expect(none).toEqual({ success: true, data: { t: "tokens", tokens: undefined } });
  });

  it("turns a bad tokens frame into one without tokens", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const broken = structuredClone(fixtureJson.status.tokens) as {
      plan: { rate: { input: number } };
    };
    broken.plan.rate.input = -1;

    const result = parseFrame({ t: "tokens", tokens: broken });

    expect(result).toEqual({ success: true, data: { t: "tokens", tokens: undefined } });
    expect(error).toHaveBeenCalled();
    error.mockRestore();
  });
});

describe("stageDynamicSchema", () => {
  it("stays strict after omit, so a key outside the split is a parse error", () => {
    expect(stageDynamicSchema.safeParse(dynamicPart(server)).success).toBe(true);
    expect(stageDynamicSchema.safeParse({ ...dynamicPart(server), unforeseen: 1 }).success).toBe(
      false,
    );
  });

  it("covers every stage key the immutable mask does not", () => {
    const all = Object.keys(stageSummarySchema.shape).sort();
    const dynamic = Object.keys(stageDynamicSchema.shape);

    expect([...dynamic, ...IMMUTABLE_STAGE_KEYS].sort()).toEqual(all);
    expect(dynamic.filter((key) => (IMMUTABLE_STAGE_KEYS as string[]).includes(key))).toEqual([]);
  });
});
