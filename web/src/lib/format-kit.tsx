import fixtureJson from "@/api/fixtures/snapshot.json";
import { snapshotSchema, type StageSummary } from "@/api/schema";

export const fixture = snapshotSchema.parse(fixtureJson);
export const stages = new Map(fixture.status.stages.map((stage) => [stage.id, stage]));
const template = fixture.status.stages[0];
export const clientFailure = stages.get("client")!.failure_info!;

export function stage(overrides: Partial<StageSummary> = {}): StageSummary {
  return { ...template, ...overrides };
}
