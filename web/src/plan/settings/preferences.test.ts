import { createStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it } from "vitest";

import helloJson from "@/plan/api/fixtures/hello.json";
import { planMessageSchema } from "@/plan/api/schema";
import { serverInfoAtom } from "@/plan/state/atoms";
import { autoRebuildAtom, readingFaceAtom, readingSizeAtom, sourceMarksAtom } from "./preferences";

const hello = planMessageSchema.parse(helloJson);
if (hello.type !== "hello") throw new Error("hello.json is not a hello frame");
const server = hello.server;

function readyStore(projectKey: string) {
  const store = createStore();
  store.set(serverInfoAtom, { ...server, project_key: projectKey });
  return store;
}

beforeEach(() => localStorage.clear());
afterEach(() => localStorage.clear());

it("defaults to Source Serif 4 at 19px with both reading toggles enabled", () => {
  const store = readyStore("settings-defaults");

  expect(store.get(readingFaceAtom)).toBe("source-serif");
  expect(store.get(readingSizeAtom)).toBe(19);
  expect(store.get(sourceMarksAtom)).toBe(true);
  expect(store.get(autoRebuildAtom)).toBe(true);
});

it("persists reading preferences per project and restores them in a new store", () => {
  const store = readyStore("settings-round-trip");

  store.set(readingFaceAtom, "literata");
  store.set(readingSizeAtom, 21);
  store.set(sourceMarksAtom, false);
  store.set(autoRebuildAtom, false);

  expect(localStorage.getItem("loom-plan:settings-round-trip:reading-face")).toBe('"literata"');
  expect(localStorage.getItem("loom-plan:settings-round-trip:reading-size")).toBe("21");
  expect(localStorage.getItem("loom-plan:settings-round-trip:source-marks")).toBe("false");
  expect(localStorage.getItem("loom-plan:settings-round-trip:auto-rebuild-summaries")).toBe(
    "false",
  );
  const reloaded = readyStore("settings-round-trip");
  const unsubscribe = [
    reloaded.sub(readingFaceAtom, () => {}),
    reloaded.sub(readingSizeAtom, () => {}),
    reloaded.sub(sourceMarksAtom, () => {}),
    reloaded.sub(autoRebuildAtom, () => {}),
  ];
  try {
    expect(reloaded.get(readingFaceAtom)).toBe("literata");
    expect(reloaded.get(readingSizeAtom)).toBe(21);
    expect(reloaded.get(sourceMarksAtom)).toBe(false);
    expect(reloaded.get(autoRebuildAtom)).toBe(false);
  } finally {
    unsubscribe.forEach((stop) => stop());
  }
  const otherProject = readyStore("settings-other-project");
  expect(otherProject.get(readingFaceAtom)).toBe("source-serif");
  expect(otherProject.get(readingSizeAtom)).toBe(19);
  expect(otherProject.get(sourceMarksAtom)).toBe(true);
  expect(otherProject.get(autoRebuildAtom)).toBe(true);
});

it.each(['"unknown"', "19", "null", "{"])(
  "falls back to the default face for invalid stored value %s",
  (stored) => {
    const projectKey = `settings-invalid-face-${stored}`;
    localStorage.setItem(`loom-plan:${projectKey}:reading-face`, stored);
    const store = readyStore(projectKey);

    expect(store.get(readingFaceAtom)).toBe("source-serif");
  },
);

it.each(["18", '"19"', "null", "{"])("falls back to 19px for invalid stored size %s", (stored) => {
  const projectKey = `settings-invalid-size-${stored}`;
  localStorage.setItem(`loom-plan:${projectKey}:reading-size`, stored);
  const store = readyStore(projectKey);

  expect(store.get(readingSizeAtom)).toBe(19);
});

it.each(['"false"', "0", "null", "{"])(
  "falls back to enabled toggles for invalid stored value %s",
  (stored) => {
    const projectKey = `settings-invalid-toggles-${stored}`;
    localStorage.setItem(`loom-plan:${projectKey}:source-marks`, stored);
    localStorage.setItem(`loom-plan:${projectKey}:auto-rebuild-summaries`, stored);
    const store = readyStore(projectKey);

    expect(store.get(sourceMarksAtom)).toBe(true);
    expect(store.get(autoRebuildAtom)).toBe(true);
  },
);

it("keeps edits in memory before the first hello without writing localStorage", () => {
  const store = createStore();

  store.set(readingFaceAtom, "piazzolla");
  store.set(readingSizeAtom, 17);
  store.set(sourceMarksAtom, false);
  store.set(autoRebuildAtom, false);

  expect(store.get(readingFaceAtom)).toBe("piazzolla");
  expect(store.get(readingSizeAtom)).toBe(17);
  expect(store.get(sourceMarksAtom)).toBe(false);
  expect(store.get(autoRebuildAtom)).toBe(false);
  expect(localStorage.length).toBe(0);

  store.set(serverInfoAtom, { ...server, project_key: "settings-first-hello" });
  expect(store.get(readingFaceAtom)).toBe("source-serif");
  expect(store.get(readingSizeAtom)).toBe(19);
  expect(store.get(sourceMarksAtom)).toBe(true);
  expect(store.get(autoRebuildAtom)).toBe(true);
});
