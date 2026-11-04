import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import { isKickoffSession, kickoffSessionAtom } from "@/plan/new/kickoff-state";

it.each([
  { session: null, requested: null, expected: false },
  { session: null, requested: "kickoff", expected: false },
  { session: "kickoff", requested: null, expected: false },
  { session: "kickoff", requested: "kickoff", expected: true },
  { session: "ordinary-chat", requested: "kickoff", expected: false },
])("identifies session $session when the address names $requested", (input) => {
  expect(isKickoffSession(input.session, input.requested)).toBe(input.expected);
});

it("keeps the started session in its own store", () => {
  const tab = createStore();
  const otherTab = createStore();

  tab.set(kickoffSessionAtom, "tab-kickoff");

  expect(tab.get(kickoffSessionAtom)).toBe("tab-kickoff");
  expect(otherTab.get(kickoffSessionAtom)).toBeNull();
});
