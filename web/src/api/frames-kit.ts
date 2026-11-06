import fixtureJson from "@/api/fixtures/snapshot.json";
import { IMMUTABLE_STAGE_KEYS, parseFrame, type StateFrame, type TickFrame } from "@/api/frames";
import { snapshotSchema, type StageSummary } from "@/api/schema";

/** The parsed shared snapshot fixture. */
export const fixture = snapshotSchema.parse(fixtureJson);

/** The part of a stage that a `state` message carries: everything but the immutable keys. */
export function dynamicPart(stage: StageSummary): Record<string, unknown> {
  const immutable: readonly string[] = IMMUTABLE_STAGE_KEYS;
  return Object.fromEntries(Object.entries(stage).filter(([key]) => !immutable.includes(key)));
}

function parsed(raw: unknown) {
  const result = parseFrame(raw);
  if (!result.success) throw new Error(`test frame rejected: ${JSON.stringify(result.issues)}`);
  return result.data;
}

/** A validated `state` frame; throws when the body is not one. */
export function stateFrame(body: object): StateFrame {
  const frame = parsed({ t: "state", at: "2026-09-04T12:00:01Z", ...body });
  if (frame.t !== "state") throw new Error("not a state frame");
  return frame;
}

/** A validated `tick` frame; throws when the body is not one. */
export function tickFrame(body: object): TickFrame {
  const frame = parsed({
    t: "tick",
    at: "2026-09-04T12:00:01Z",
    tick_age: 5,
    stages: {},
    ...body,
  });
  if (frame.t !== "tick") throw new Error("not a tick frame");
  return frame;
}
