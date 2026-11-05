import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import { RouterProvider, createMemoryRouter } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import fixture from "@/plan/api/fixtures/chat-snapshot.json";
import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema, offerViewSchema } from "@/plan/api/schema";
import { chatSnapshotSchema, type SessionView } from "@/plan/api/schema-chat";
import { Dock } from "@/plan/dock";
import { OfferButton } from "@/plan/offers/offer-button";
import {
  authoringAtom,
  connectionAtom,
  projectAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import { snapshotAtom } from "./model";
import { wroteTurns } from "./start-chat-test-kit";

// The test file owns its `vi.mock` calls (vitest hoists them per file); this kit holds the rest.
export const offer = offerViewSchema.parse(hello.checklists[0]!.primary);
const checklist = checklistViewSchema.parse(hello.checklists[0]);
const base = chatSnapshotSchema.parse(fixture).session!;
let serial = 0;

export function session(fields: Partial<SessionView> = {}): SessionView {
  return { ...base, state: "idle", feature: "alpha", ...fields };
}

export function setup(path: string, current: SessionView | null, selected = offer) {
  const store = createStore();
  store.set(serverInfoAtom, { ...hello.server, project_key: `home-${++serial}`, read_only: [] });
  store.set(youAtom, hello.you);
  store.set(authoringAtom, { holder: { ...hello.you, away_since: null, frees_at: null } });
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: Date.now() });
  store.set(projectAtom, {
    checklists: [
      { ...checklist, feature: "alpha" },
      { ...checklist, feature: "beta" },
    ],
    standalone: [{ slug: "solo", path: "doc/plans/PLAN-solo.md", state: "written", title: "Solo" }],
    moves: [],
  });
  store.set(snapshotAtom, {
    session: current,
    turns: current ? wroteTurns : [],
    queue: [],
    previous: null,
  });
  const router = createMemoryRouter(
    [
      {
        path: "*",
        element: <Dock />,
      },
    ],
    { initialEntries: [path] },
  );
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  // The offer mounts after the registration effect, as it does in the app.
  render(
    <Provider store={store}>
      <OfferButton offer={selected} />
    </Provider>,
  );
  return { store, router };
}

export const panel = () => screen.getByRole("complementary", { name: "Chat" });
export const notice = (title: string) =>
  within(panel()).queryByRole("link", { name: `A session in ${title} needs you` });
export const clickOffer = async (selected = offer) => {
  fireEvent.click(screen.getByRole("button", { name: selected.label }));
  await act(async () => {});
};

/** Registers the per-test reset; call once at the top of the test file. */
export function installHomeLifecycle(): void {
  beforeEach(() => {
    localStorage.clear();
    vi.clearAllMocks();
    vi.spyOn(window, "confirm").mockReturnValue(false);
    vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
      matches: query.includes("min-width"),
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: () => true,
    }));
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });
}
