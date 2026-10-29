import { createStore } from "jotai/vanilla";
import { expect, it } from "vitest";

import type { KickoffView } from "@/plan/api/schema-kickoff";
import { isKickoffSession, kickoffSessionAtom } from "@/plan/new/kickoff-state";

const view: KickoffView = {
  session: "shared-kickoff",
  target: null,
  shape: {
    size: null,
    name: null,
    proposal: false,
    problem: "",
    decisions: [],
    questions: [],
    sections: [],
  },
};

it.each([
  { session: null, tab: null, view: null, expected: false },
  { session: null, tab: "tab-kickoff", view, expected: false },
  { session: "tab-kickoff", tab: "tab-kickoff", view: null, expected: true },
  { session: "shared-kickoff", tab: null, view, expected: true },
  { session: "shared-kickoff", tab: "other-kickoff", view, expected: true },
  { session: "ordinary-chat", tab: "tab-kickoff", view, expected: false },
  { session: "ordinary-chat", tab: null, view: null, expected: false },
])("identifies kickoff session $session with tab $tab", ({ session, tab, view, expected }) => {
  expect(isKickoffSession(session, tab, view)).toBe(expected);
});

it("keeps the started session in its own store", () => {
  const tab = createStore();
  const otherTab = createStore();

  tab.set(kickoffSessionAtom, "tab-kickoff");

  expect(tab.get(kickoffSessionAtom)).toBe("tab-kickoff");
  expect(otherTab.get(kickoffSessionAtom)).toBeNull();
});
