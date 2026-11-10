import { describe, expect, it } from "vitest";
import fixture from "@/plan/api/fixtures/trace.json";
import settleCommit from "@/plan/api/fixtures/trace-settle-commit.json";
import settleSaved from "@/plan/api/fixtures/trace-settle-saved.json";
import { planMessageSchema, socketMessageSchema } from "./schema";
import {
  findingStatusSchema,
  settleResultSchema,
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

  it("parses the settle answer the server test writes, with its queued commit", () => {
    expect(settleResultSchema.parse(settleSaved)).toEqual(settleSaved);
    const reason = "the commit worker could not start";
    const unqueued = { ...settleSaved, commit: { state: "not_committed", reason } };
    expect(settleResultSchema.parse(unqueued)).toEqual(unqueued);
    const done = { ...settleSaved, commit: { state: "committed", job: "3f9a1c2e-7" } };
    expect(settleResultSchema.safeParse(done).success).toBe(false);
    const numbered = { ...settleSaved, commit: { state: "pending", job: 7 } };
    expect(settleResultSchema.safeParse(numbered).success).toBe(false);
    const old = { ...settleSaved, commit: null, commit_error: null };
    expect(settleResultSchema.safeParse(old).success).toBe(false);
    expect(settleResultSchema.safeParse({ ...settleSaved, extra: 1 }).success).toBe(false);
    const conflict = { result: "conflict", current_text: "x", current_hash: "sha256:x" };
    expect(settleResultSchema.parse(conflict)).toEqual(conflict);
  });

  it("parses the settle_commit frame the server test writes as a plan message", () => {
    expect(planMessageSchema.parse(settleCommit)).toEqual(settleCommit);
    expect(socketMessageSchema.parse(settleCommit)).toEqual(settleCommit);
    const refused = { ...settleCommit, state: "not_committed", commit: null, reason: "a merge" };
    expect(planMessageSchema.parse(refused)).toEqual(refused);
    expect(planMessageSchema.safeParse({ ...settleCommit, state: "pending" }).success).toBe(false);
    expect(planMessageSchema.safeParse({ ...settleCommit, extra: 1 }).success).toBe(false);
    expect(planMessageSchema.safeParse({ ...settleCommit, hash: null }).success).toBe(false);
  });

  it("parses trace_changed as a plan message", () => {
    const frame = { type: "trace_changed", feature: "demo" };
    expect(traceMessageSchemas[0].parse(frame)).toEqual(frame);
    expect(planMessageSchema.parse(frame)).toEqual(frame);
    expect(socketMessageSchema.parse(frame)).toEqual(frame);
    expect(planMessageSchema.safeParse({ ...frame, extra: 1 }).success).toBe(false);
  });
});
