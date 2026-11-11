import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import background from "@/plan/api/fixtures/background.json";
import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import { backgroundMessageSchemas } from "@/plan/api/schema-background";
import { chatSnapshotSchema } from "@/plan/api/schema-chat";
import { backgroundAtom } from "@/plan/state/atoms";

import {
  backgroundEntryAtom,
  chatBackgroundAtom,
  isUnended,
  unendedBackgroundCountAtom,
} from "./background-tasks";
import { snapshotAtom } from "./model";

const frame = (() => {
  const { type: _type, ...rest } = backgroundMessageSchemas[0].parse(background);
  return rest;
})();

function setup() {
  const store = createStore();
  store.set(snapshotAtom, chatSnapshotSchema.parse(snapshotFixture));
  return store;
}

describe("chat background selectors", () => {
  it("holds the session's tasks and counts the unended ones", () => {
    const store = setup();
    store.set(backgroundAtom, frame);
    expect(store.get(chatBackgroundAtom)).toHaveLength(5);
    expect(store.get(unendedBackgroundCountAtom)).toBe(3);
  });

  it("ignores a frame for another session and a missing frame", () => {
    const store = setup();
    expect(store.get(chatBackgroundAtom)).toEqual([]);
    store.set(backgroundAtom, { ...frame, session: "another-session" });
    expect(store.get(chatBackgroundAtom)).toEqual([]);
    expect(store.get(unendedBackgroundCountAtom)).toBe(0);
  });

  it("ignores every frame while the chat has no session", () => {
    const store = createStore();
    store.set(backgroundAtom, frame);
    expect(store.get(chatBackgroundAtom)).toEqual([]);
  });

  it("counts running, tool-wait and stalled as unended", () => {
    expect(["running", "tool-wait", "stalled"].map((s) => isUnended(s as never))).toEqual([
      true,
      true,
      true,
    ]);
    expect(["done", "failed", "stopped"].map((s) => isUnended(s as never))).toEqual([
      false,
      false,
      false,
    ]);
  });

  it("selects a task by tool_use_id and keeps its identity across unrelated changes", () => {
    const store = setup();
    store.set(backgroundAtom, frame);
    const entry = backgroundEntryAtom("toolu_bg1");
    expect(backgroundEntryAtom("toolu_bg1")).toBe(entry);
    const first = store.get(entry);
    expect(first).toMatchObject({ id: "a-run", state: "running" });
    // Mount a subscriber so the select atom caches its previous value.
    const seen: unknown[] = [];
    store.sub(entry, () => seen.push(store.get(entry)));

    const [agent, ...rest] = frame.tasks;
    store.set(backgroundAtom, {
      ...frame,
      tasks: [{ ...agent! }, ...rest.map((t) => ({ ...t, steps: 99 }))],
    });
    expect(store.get(entry)).toBe(first);
    expect(seen).toEqual([]);

    store.set(backgroundAtom, {
      ...frame,
      tasks: [{ ...agent!, state: "done" }, ...rest],
    });
    expect(store.get(entry)).toMatchObject({ state: "done" });
    expect(store.get(backgroundEntryAtom("missing"))).toBeNull();
  });
});
