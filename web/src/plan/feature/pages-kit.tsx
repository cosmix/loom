import { render } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactElement } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import hello from "@/plan/api/fixtures/hello.json";
import runStateFixture from "@/plan/api/fixtures/run-state.json";
import runStripFixture from "@/plan/api/fixtures/run-strip.json";
import serverInfoFixture from "@/plan/api/fixtures/server-info.json";
import {
  authoringViewSchema,
  checklistViewSchema,
  serverInfoSchema,
  type ChecklistView,
  type OfferKind,
  type PlanMessage,
} from "@/plan/api/schema";
import { runStateSchema, runStripSchema } from "@/plan/api/schema-run";
import type { connectPlanSocket } from "@/plan/api/socket";
import { installRunSubscription, runStateAtom, runStripAtom } from "@/plan/run/use-run";
import {
  authoringAtom,
  planSocketAtom,
  projectAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";
import type { Store } from "@/state/store";

/** Test kit for the Pressure test and Run pages: a feature `atlas` and a page mounted on its route. */

export function offer(
  kind: OfferKind,
  label: string,
  target: string | null = null,
  id: string = kind,
) {
  return { id, kind, label, target, feature: "atlas" };
}

export function planRow(slug: string, state: string, pressure_date: string | null = null) {
  return {
    slug,
    path: `doc/plans/atlas/PLAN-${slug}.md`,
    title: `Plan ${slug}`,
    state,
    pressure_date,
    listed: true,
  };
}

export function atlas(patch: Record<string, unknown>): ChecklistView {
  return checklistViewSchema.parse({
    ...checklistFixture.checklists[0]!,
    feature: "atlas",
    primary: null,
    secondary: [],
    items: [],
    ...patch,
  });
}

/** The run state of the server fixture with `patch` over it: a run of the fixture's own plan. */
export function runState(patch: Record<string, unknown> = {}) {
  return runStateSchema.parse({ ...runStateFixture, ...patch });
}

export function runStrip(patch: Record<string, unknown> = {}) {
  return runStripSchema.parse({ ...runStripFixture, ...patch });
}

export type Seed = {
  run?: ReturnType<typeof runState> | null;
  strip?: ReturnType<typeof runStrip> | null;
  /** Who holds authoring: this browser, or Ada in another (offers are then blocked). */
  holder?: "me" | "other";
};

/** Mount `page` at `path` (`/f/atlas/...`) with the checklist and the seeded run atoms. */
export function renderPage(
  page: ReactElement,
  path: string,
  checklist: ChecklistView,
  seed: Seed = {},
) {
  const store = createStore();
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...serverInfoFixture, read_only: [] }));
  store.set(youAtom, hello.you);
  const held = { ...hello.you, away_since: null, frees_at: null };
  const holders = { me: held, other: { ...held, public_id: "other" } };
  store.set(
    authoringAtom,
    authoringViewSchema.parse({ holder: seed.holder ? holders[seed.holder] : null }),
  );
  store.set(projectAtom, { checklists: [checklist], standalone: [], moves: [] });
  store.set(runStateAtom, seed.run ?? null);
  store.set(runStripAtom, seed.strip ?? null);
  const router = createMemoryRouter([{ path: "/f/:feature/*", element: page }], {
    initialEntries: [path],
  });
  const view = render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { ...view, store };
}

/**
 * Install the run subscription on a socket that delivers run strips on demand: what the server
 * pushes while a page is open. Returns the push and the teardown.
 */
export function stripSocket(store: Store) {
  const handlers = new Set<(strip: PlanMessage) => void>();
  const socket: ReturnType<typeof connectPlanSocket> = {
    send: () => true,
    dispose: () => handlers.clear(),
    subscribe: (type, handler) => {
      const listener = (frame: PlanMessage) => {
        if (frame.type === type) handler(frame as Extract<PlanMessage, { type: typeof type }>);
      };
      handlers.add(listener);
      return () => handlers.delete(listener);
    },
    onResync: () => () => {},
  };
  store.set(planSocketAtom, socket);
  const dispose = installRunSubscription(store);
  return {
    push: (strip: ReturnType<typeof runStrip>) =>
      handlers.forEach((handler) => handler({ type: "run_strip", ...strip })),
    dispose,
  };
}
