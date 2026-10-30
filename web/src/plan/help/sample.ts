import { serverInfoSchema } from "@/plan/api/schema";

/** The seven planning steps with one done, one gap and one active step, shared by every scene. */
export const SAMPLE_STEPS = [
  { number: 1, name: "Vision", mark: "done", count: null, label: "Vision" },
  { number: 2, name: "Product spec", mark: "gap", count: null, label: "Product spec" },
  { number: 3, name: "Metaplan", mark: "active", count: null, label: "Metaplan" },
  { number: 4, name: "Plans", mark: "optional", count: null, label: "Plans" },
  { number: 5, name: "Plan review", mark: "optional", count: null, label: "Plan review" },
  { number: 6, name: "Run review", mark: "optional", count: null, label: "Run review" },
  { number: 7, name: "Execute", mark: "optional", count: null, label: "Execute" },
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
