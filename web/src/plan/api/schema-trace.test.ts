import { describe, expect, it } from "vitest";
import fixture from "@/plan/api/fixtures/trace.json";
import { planMessageSchema, socketMessageSchema } from "./schema";
import {
  findingStatusSchema,
  settledVersionSchema,
  traceMessageSchemas,
  traceViewSchema,
  unresolvedSchema,
} from "./schema-trace";

describe("trace wire schemas", () => {
  it("parses the trace fixture the server test writes", () => {
    expect(traceViewSchema.parse(fixture)).toEqual(fixture);
  });

  it("carries every part of the trace in the fixture", () => {
    const view = traceViewSchema.parse(fixture);
    expect(view.documents.map((doc) => doc.settle)).toContain("revised");
    expect(view.documents.some((doc) => doc.bundle.length > 0)).toBe(true);
    expect(view.headings.some((h) => h.covered_by.some((c) => c.kind === "stage"))).toBe(true);
    expect(view.findings.map((f) => f.finding.status.kind)).toEqual(["open", "dismissed"]);
    expect(view.frontier.length).toBeLessThanOrEqual(5);
  });

  it("rejects extra fields at every level", () => {
    expect(traceViewSchema.safeParse({ ...fixture, extra: true }).success).toBe(false);
    const doc = { ...fixture.documents[0], extra: 1 };
    const documents = [doc, ...fixture.documents.slice(1)];
    expect(traceViewSchema.safeParse({ ...fixture, documents }).success).toBe(false);
  });

  it("rejects an unknown settle state and a status kind", () => {
    const doc = { ...fixture.documents[0], settle: "accepted" };
    const documents = [doc, ...fixture.documents.slice(1)];
    expect(traceViewSchema.safeParse({ ...fixture, documents }).success).toBe(false);
    expect(findingStatusSchema.safeParse({ kind: "closed", detail: null }).success).toBe(false);
    expect(findingStatusSchema.safeParse({ kind: "open", detail: null }).success).toBe(true);
  });

  it("parses the settled-version and unresolved answers", () => {
    expect(settledVersionSchema.parse({ commit: null })).toEqual({ commit: null });
    expect(settledVersionSchema.safeParse({ commit: 5 }).success).toBe(false);
    const unresolved = {
      error: "unresolved",
      message: "confirm",
      my_calls: 1,
      open_questions: 2,
    };
    expect(unresolvedSchema.parse(unresolved)).toEqual(unresolved);
  });

  it("parses trace_changed as a plan message", () => {
    const frame = { type: "trace_changed", feature: "demo" };
    expect(traceMessageSchemas[0].parse(frame)).toEqual(frame);
    expect(planMessageSchema.parse(frame)).toEqual(frame);
    expect(socketMessageSchema.parse(frame)).toEqual(frame);
    expect(planMessageSchema.safeParse({ ...frame, extra: 1 }).success).toBe(false);
  });
});
