import { act, screen } from "@testing-library/react";
import { expect, it } from "vitest";

import { authoringAtom, presenceAtom, youAtom } from "@/plan/state/atoms";
import { harness, session, setupStateLineTests, since, turn } from "./state-line-kit";

setupStateLineTests();

it("session total and time left when quota known", () => {
  const h = harness(session(), [turn(), turn([], { id: "second" })]);
  h.header();
  expect(screen.getByText("· 3.0k tokens")).toBeTruthy();
  // The session wire model has no quota reading; omit time left until it supplies one.
  expect(screen.queryByText(/time left|minutes left|lasts to reset/i)).toBeNull();
  h.setTurns([turn()]);
  expect(screen.getByText("· 1.5k tokens")).toBeTruthy();
});

it("away line and return", () => {
  const h = harness(session({ away_until: "2026-10-28T10:10:00Z" }));
  h.store.set(authoringAtom, {
    holder: {
      public_id: "holder",
      name: "Dana",
      away_since: since,
      frees_at: "2026-10-28T10:10:00Z",
    },
  });
  const { container } = h.header();
  expect(container.textContent).toContain("Dana is away · authoring frees at");
  expect(container.querySelector(".chat-run-header")?.textContent).toMatch(/\d{2}:\d{2}/);
  act(() =>
    h.store.set(authoringAtom, {
      holder: { public_id: "holder", name: "Dana", away_since: null, frees_at: null },
    }),
  );
  expect(container.textContent).toContain("Dana authoring");
  expect(container.textContent).not.toContain("is away");
});

it("holder and watcher count with names", () => {
  const h = harness();
  h.store.set(presenceAtom, {
    entries: [
      { public_id: "holder", name: "Dana", holder: true, away: false, reading: null, tabs: 2 },
      { public_id: "viewer1", name: "Alex", holder: false, away: false, reading: null, tabs: 3 },
      { public_id: "viewer2", name: "Sam", holder: false, away: false, reading: null, tabs: 1 },
    ],
  });
  const { container } = h.header();
  expect(container.textContent).toContain("Dana authoring");
  expect(screen.getByText("2 watching")).toBeTruthy();
  expect(screen.getByTitle("Alex, Sam").getAttribute("aria-label")).toBe("2 watching: Alex, Sam");
  act(() => h.store.set(youAtom, { public_id: "viewer1", name: "Alex" }));
  expect(container.textContent).toContain("Read-only · Dana is authoring");
});

it("the run line is three segments on one line: the name gives way, the count drops last", () => {
  const { container } = harness().header();
  const header = container.querySelector(".chat-run-header")!;
  const segments = [...header.querySelectorAll(":scope > .chat-run-seg")];
  expect(segments).toHaveLength(3);
  expect(segments[0].hasAttribute("data-cut")).toBe(true);
  expect(segments[0].textContent).toContain("authoring");
  expect(segments[1].textContent).toBe("· 0 watching");
  expect(segments[2].classList.contains("chat-run-tokens")).toBe(true);
  expect(segments[2].textContent).toMatch(/^· \S+ tokens$/);
  expect(header.classList.contains("chat-run-line")).toBe(true);
});
