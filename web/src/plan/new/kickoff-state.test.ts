import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import { isKickoffSession, kickoffSessionAtom, kindOf } from "@/plan/new/kickoff-state";

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

it.each([
  { size: "plan", offer: "kickoff", expected: "feature" },
  { size: "feature", offer: "kickoff-small", expected: "plan" },
  { size: null, offer: "kickoff", expected: "feature" },
  { size: null, offer: "kickoff-small", expected: "plan" },
  { size: "plan", offer: null, expected: "plan" },
  { size: "feature", offer: "something-else", expected: "feature" },
  { size: null, offer: null, expected: null },
] as const)("reads size $size under offer $offer as $expected", ({ size, offer, expected }) => {
  expect(kindOf(size, offer)).toBe(expected);
});
