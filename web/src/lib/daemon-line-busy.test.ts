import { describe, expect, it } from "vitest";

import type { Busy } from "../api/schema";
// A namespace import keeps this file loadable whatever format.ts exports.
import * as format from "./format";

const NOW_MS = Date.parse("2026-11-08T12:00:00.000Z");

function busySince(secondsAgo: number, op: Busy["op"], what: string, ceilingSecs: number): Busy {
  return {
    op,
    what,
    stage_id: "stage-a",
    started_at: new Date(NOW_MS - secondsAgo * 1000).toISOString(),
    ceiling_secs: ceilingSecs,
    global_ceiling_secs: 1800,
  };
}

const BRIEF = "generating the brief for stage-a";

describe("daemonLine busy", () => {
  it("names the operation under its ceiling", () => {
    const busy = busySince(75, "brief-generation", BRIEF, 300);
    const line = format.daemonLine("running", 2, null, busy, NOW_MS);
    expect(line.text).toBe("daemon running");
    expect(line.detail).toContain(`busy ${BRIEF}`);
    expect(line.text).not.toContain("stalled");
    expect(line.tone).toBe("completed");
  });

  it("warns past the operation ceiling", () => {
    const busy = busySince(400, "brief-generation", BRIEF, 300);
    const line = format.daemonLine("running", 2, null, busy, NOW_MS);
    expect(line.text).toBe("daemon running");
    expect(line.detail).toContain(`busy ${BRIEF}`);
    expect(line.tone).toBe("warning");
  });

  it("reports a stall past the global ceiling", () => {
    const busy = busySince(1900, "provisioning", "provisioning stage-a", 600);
    const line = format.daemonLine("running", 2, null, busy, NOW_MS);
    expect(line.text).toBe(`loop stalled ${format.formatElapsed(1900)}`);
    expect(line.tone).toBe("warning");
  });
});
