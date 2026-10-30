import { createStore } from "jotai";

import { serverInfoSchema } from "@/plan/api/schema";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

/**
 * The six planning steps as the server publishes them: `name` is the step, `label` and `count`
 * are what the spine row shows. One done, one gap and one active step, shared by every scene.
 */
export const SAMPLE_STEPS = [
  { number: 1, name: "Vision", mark: "done", count: "1", label: "Vision" },
  { number: 2, name: "Product spec", mark: "gap", count: "5/6", label: "Product spec" },
  { number: 3, name: "Metaplan", mark: "active", count: "1", label: "Metaplan" },
  { number: 4, name: "Plans", mark: "optional", count: null, label: "Plans" },
  { number: 5, name: "Pressure test", mark: "optional", count: null, label: "Pressure test" },
  { number: 6, name: "Run", mark: "optional", count: null, label: "Run" },
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

/**
 * A scene store starts from these atoms, never from the host's: the sample server is writable and
 * the reader holds authoring, so no illustration shows the live server's read-only notice or holder.
 */
export function createSampleStore(): ReturnType<typeof createStore> {
  const store = createStore();
  store.set(serverInfoAtom, SAMPLE_SERVER);
  store.set(youAtom, SAMPLE_SERVER.you);
  store.set(authoringAtom, SAMPLE_HOLDER);
  return store;
}

/** The chat panel's default width and fill, so a panel scene lays out as it does in the dock. */
export const PANEL_LOOK = "w-[21.25rem] max-w-full rounded-[10px] border border-hairline bg-card";

export function ignoreAction(): void {}
