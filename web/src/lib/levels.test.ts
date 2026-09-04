import { describe, expect, it } from "vitest";

import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";
import { computeLevels, orderStages } from "@/lib/levels";

const fixture = snapshotSchema.parse(fixtureJson);
const template = fixture.status.stages[0];

function stage(id: string, dependencies: string[]): StageSummary {
  return { ...template, id, dependencies };
}

describe("stage levels", () => {
  it("matches the fixture dependency levels", () => {
    const levels = computeLevels(fixture.status.stages);

    expect(levels.get("knowledge-bootstrap")).toBe(0);
    expect(levels.get("server")).toBe(1);
    expect(levels.get("client")).toBe(1);
    expect(levels.get("docs")).toBe(1);
    expect(levels.get("design")).toBe(2);
    expect(levels.get("integration-verify")).toBe(3);
    expect(levels.get("knowledge-distill")).toBe(4);
  });

  it("orders stages by level, then plain string id", () => {
    expect(orderStages(fixture.status.stages).map((entry) => entry.stage.id)).toEqual([
      "knowledge-bootstrap",
      "client",
      "docs",
      "server",
      "design",
      "integration-verify",
      "knowledge-distill",
    ]);
  });

  it("uses the first duplicate stage in the ordered list", () => {
    const first = stage("duplicate", []);
    const second = stage("duplicate", ["missing"]);

    expect(orderStages([first, second])).toHaveLength(1);
    expect(orderStages([first, second])[0].stage).toBe(first);
  });

  it("gives a self-cycle level one", () => {
    expect(computeLevels([stage("a", ["a"])]).get("a")).toBe(1); // correction C16
  });

  it("gives a missing dependency level one", () => {
    expect(computeLevels([stage("a", ["missing"])]).get("a")).toBe(1); // correction C16
  });
});
