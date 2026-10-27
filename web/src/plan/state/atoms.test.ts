import { createStore } from "jotai/vanilla";
import { describe, expect, it } from "vitest";

import helloJson from "@/plan/api/fixtures/hello.json";
import { planMessageSchema } from "@/plan/api/schema";
import {
  authorBlockReasonAtom,
  authoringAtom,
  canAuthorAtom,
  isHolderAtom,
  serverInfoAtom,
  stoppingAtom,
  youAtom,
} from "@/plan/state/atoms";

const message = planMessageSchema.parse(helloJson);
if (message.type !== "hello") throw new Error("hello.json is not a hello frame");
const hello = message;

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

  it("uses the first read-only problem when free", () => {
    const store = readyStore();
    store.set(serverInfoAtom, hello.server);
    expect(store.get(canAuthorAtom)).toBe(false);
    expect(store.get(authorBlockReasonAtom)).toBe(
      `loom plan is in read-only mode: ${hello.server.read_only[0].message}`,
    );
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
