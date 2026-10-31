import { expect, it } from "vitest";
import fixture from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageIssue } from "@/api/schema";
import { historyEvents } from "@/components/stage-history-model";

it("uses typed outcomes even when the explanation changes", () => {
  const stage = snapshotSchema.parse(fixture).status.stages[0];
  const issue: StageIssue = {
    kind: "failure",
    failure_type: null,
    detected_at: null,
    evidence: [],
    ended_at: "2026-10-01T00:00:00Z",
    ended_by: "wording changed",
    disposition: "retried",
    dispute_id: null,
  };
  stage.history = [
    issue,
    { ...issue, ended_by: "future legacy explanation", disposition: "unknown" },
  ];
  expect(historyEvents(stage, undefined).map((event) => event.mark)).toEqual([
    "retried",
    "unknown",
  ]);
});
