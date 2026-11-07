import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import helloJson from "@/plan/api/fixtures/hello.json";
import { planMessageSchema, type ProjectChecklistView } from "@/plan/api/schema";
import {
  activeFeatureAtom,
  authorBlockReasonAtom,
  authoringAtom,
  canAuthorAtom,
  checklistAtom,
  draftTeamAtom,
  featuresAtom,
  isHolderAtom,
  kickoffAtom,
  movesAtom,
  projectAtom,
  serverInfoAtom,
  standaloneAtom,
  stoppingAtom,
  youAtom,
} from "@/plan/state/atoms";

const message = planMessageSchema.parse(helloJson);
if (message.type !== "hello") throw new Error("hello.json is not a hello frame");
const hello = message;
const demo = { ...hello.checklists[0], feature: "demo" };
const other = { ...demo, feature: "other" };
const project: ProjectChecklistView = {
  checklists: [demo, other],
  standalone: [
    {
      slug: "standalone",
      path: "doc/plans/PLAN-standalone.md",
      state: "written",
      title: "Standalone",
      last_change: null,
      has_summary: false,
    },
  ],
  moves: [{ id: "move", kind: "move_into_feature", label: "Move plan", target: null }],
};

describe("project atoms", () => {
  it("starts without a project, selection, kickoff or draft team", () => {
    const store = createStore();
    expect(store.get(projectAtom)).toBeNull();
    expect(store.get(activeFeatureAtom)).toBeNull();
    expect(store.get(checklistAtom)).toBeNull();
    expect(store.get(featuresAtom)).toEqual([]);
    expect(store.get(standaloneAtom)).toEqual([]);
    expect(store.get(movesAtom)).toEqual([]);
    expect(store.get(kickoffAtom)).toBeNull();
    expect(store.get(draftTeamAtom)).toBeNull();
  });

  it("derives the project lists and follows project replacements", () => {
    const store = createStore();
    store.set(projectAtom, project);
    expect(store.get(featuresAtom)).toBe(project.checklists);
    expect(store.get(standaloneAtom)).toBe(project.standalone);
    expect(store.get(movesAtom)).toBe(project.moves);
    store.set(projectAtom, null);
    expect(store.get(featuresAtom)).toEqual([]);
    expect(store.get(standaloneAtom)).toEqual([]);
    expect(store.get(movesAtom)).toEqual([]);
  });

  it("selects the explicit active feature and keeps it across project updates", () => {
    const store = createStore();
    store.set(projectAtom, project);
    store.set(activeFeatureAtom, other.feature);
    expect(store.get(checklistAtom)).toBe(other);
    store.set(projectAtom, null);
    expect(store.get(checklistAtom)).toBeNull();
    expect(store.get(activeFeatureAtom)).toBe(other.feature);
    store.set(projectAtom, project);
    expect(store.get(checklistAtom)).toBe(other);
  });

  it("falls back to the sole checklist when no active feature matches", () => {
    const store = createStore();
    store.set(projectAtom, { ...project, checklists: [demo] });
    expect(store.get(checklistAtom)).toBe(demo);
    store.set(activeFeatureAtom, "missing");
    expect(store.get(checklistAtom)).toBe(demo);
  });

  it("returns null when multiple checklists have no matching selection", () => {
    const store = createStore();
    store.set(projectAtom, project);
    expect(store.get(checklistAtom)).toBeNull();
    store.set(activeFeatureAtom, "missing");
    expect(store.get(checklistAtom)).toBeNull();
  });

  it("writes a single checklist through the compatibility seam", () => {
    const store = createStore();
    store.set(projectAtom, project);
    store.set(checklistAtom, demo);
    expect(store.get(projectAtom)).toEqual({ checklists: [demo], standalone: [], moves: [] });
    expect(store.get(activeFeatureAtom)).toBe(demo.feature);
    expect(store.get(checklistAtom)).toBe(demo);
    store.set(checklistAtom, null);
    expect(store.get(projectAtom)).toEqual({ checklists: [], standalone: [], moves: [] });
    expect(store.get(checklistAtom)).toBeNull();
    expect(store.get(activeFeatureAtom)).toBe(demo.feature);
  });
});

function readyStore() {
  const store = createStore();
  store.set(youAtom, hello.you);
  store.set(serverInfoAtom, { ...hello.server, read_only: [] });
  store.set(authoringAtom, { holder: null });
  return store;
}

describe("authoring atoms", () => {
  it("allows the holder even when read-only or stopping", () => {
    const store = readyStore();
    store.set(authoringAtom, hello.authoring);
    store.set(serverInfoAtom, hello.server);
    store.set(stoppingAtom, true);
    expect(store.get(isHolderAtom)).toBe(true);
    expect(store.get(canAuthorAtom)).toBe(true);
    expect(store.get(authorBlockReasonAtom)).toBeNull();
  });

  it("names another holder", () => {
    const store = readyStore();
    store.set(authoringAtom, {
      holder: { ...hello.authoring.holder!, public_id: "other", name: "Sam" },
    });
    expect(store.get(isHolderAtom)).toBe(false);
    expect(store.get(canAuthorAtom)).toBe(false);
    expect(store.get(authorBlockReasonAtom)).toBe("Sam holds authoring");
  });

  it("allows authoring when free", () => {
    const store = readyStore();
    expect(store.get(canAuthorAtom)).toBe(true);
    expect(store.get(authorBlockReasonAtom)).toBeNull();
  });

  it("reads Read-only alone when the server has a problem, the banner carrying the rest", () => {
    const store = readyStore();
    store.set(serverInfoAtom, hello.server);
    expect(store.get(canAuthorAtom)).toBe(false);
    expect(store.get(authorBlockReasonAtom)).toBe("Read-only");
  });

  it("blocks authoring while stopping", () => {
    const store = readyStore();
    store.set(stoppingAtom, true);
    expect(store.get(canAuthorAtom)).toBe(false);
    expect(store.get(authorBlockReasonAtom)).toBe("loom plan is stopping");
  });

  it("waits for initial server and authoring state", () => {
    expect(createStore().get(canAuthorAtom)).toBe(false);
  });
});
