import { serverInfoSchema } from "@/plan/api/schema";

/** The seven planning steps with one done, one gap and one active step, shared by every scene. */
export const SAMPLE_STEPS = [
  { number: 1, name: "Vision", mark: "done" },
  { number: 2, name: "Product spec", mark: "gap" },
  { number: 3, name: "Metaplan", mark: "active" },
  { number: 4, name: "Plans", mark: "optional" },
  { number: 5, name: "Plan review", mark: "optional" },
  { number: 6, name: "Run review", mark: "optional" },
  { number: 7, name: "Execute", mark: "optional" },
] as const;

export const SAMPLE_SERVER = serverInfoSchema.parse({
  project: "Atlas",
  project_key: "atlas-help",
  port: 7272,
  remote: false,
  read_only: [],
  stopping: false,
  you: { public_id: "atlas-help-reader", name: "Alex" },
});

/** The reader holding authoring, as the server publishes it: the session's `holder` is its name. */
export const SAMPLE_HOLDER = {
  holder: { ...SAMPLE_SERVER.you, away_since: null, frees_at: null },
};

export function ignoreAction(): void {}
