import { cleanup, render, screen, within } from "@testing-library/react";
import { createStore, Provider, type ExtractAtomValue } from "jotai";
import { StrictMode } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { HomeRoute, resetHomeAutoOpen } from "@/plan/home/home";
import * as atoms from "@/plan/state/atoms";

type Drafts = ExtractAtomValue<typeof atoms.kickoffDraftsAtom>;

const A = "11111111-1111-4111-8111-111111111111";
const B = "22222222-2222-4222-8222-222222222222";
const drafts: Drafts = [
  {
    session: A,
    name: "microvm-test-env",
    size: "feature",
    problem: "Tests need isolation.",
    started_at: "2026-11-04T09:30:00Z",
    updated_at: "2026-11-04T10:50:00Z",
    live: false,
  },
  {
    session: B,
    name: null,
    size: null,
    problem: null,
    started_at: "2026-11-04T10:47:11Z",
    updated_at: "2026-11-04T10:48:00Z",
    live: false,
  },
];

beforeEach(() => {
  resetHomeAutoOpen();
  vi.stubGlobal("fetch", vi.fn().mockReturnValue(new Promise(() => {})));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function mountHome() {
  const store = createStore();
  store.set(atoms.projectAtom, { checklists: [], standalone: [], moves: [] });
  store.set(atoms.authoringAtom, { holder: null });
  store.set(atoms.serverInfoAtom, {
    project: "loom",
    project_key: "drafts-contract",
    port: 8080,
    remote: false,
    stopping: false,
    you: { public_id: "reader", name: "Reader" },
    read_only: [],
  });
  store.set(atoms.kickoffDraftsAtom, drafts);
  const router = createMemoryRouter(
    [
      { path: "/", element: <HomeRoute /> },
      { path: "/new", element: <h1>Kickoff conversation</h1> },
    ],
    { initialEntries: ["/"] },
  );
  render(
    <StrictMode>
      <Provider store={store}>
        <RouterProvider router={router} />
      </Provider>
    </StrictMode>,
  );
}

it("Home lists unfinished drafts by name or as Untitled", () => {
  mountHome();
  const shelf = within(screen.getByRole("region", { name: "Unfinished" }));
  const named = shelf.getByRole("link", { name: /microvm-test-env/ });
  expect(named.getAttribute("href")).toBe("/new?session=" + A);
  const untitled = shelf.getByRole("link", { name: /^Untitled · / });
  expect(untitled.getAttribute("href")).toBe("/new?session=" + B);
});
