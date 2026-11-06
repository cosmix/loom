import { act, cleanup, render, waitFor } from "@testing-library/react";
import { Provider, createStore, useAtomValue } from "jotai";
import { createElement, StrictMode } from "react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import snapshotFixture from "@/plan/api/fixtures/chat-snapshot.json";
import type { ChecklistView, StandalonePlanView } from "@/plan/api/schema";
import { sessionViewSchema, type SessionView } from "@/plan/api/schema-chat";
import type { KickoffView } from "@/plan/api/schema-kickoff";
import { sessionAtom, snapshotAtom } from "@/plan/chat/model";
import { isActiveSession } from "@/plan/chat/session-home";
import { resetKickoffLanding, useKickoffLanding } from "@/plan/new/kickoff-landing";
import { kickoffAtom, projectAtom } from "@/plan/state/atoms";

const feature: ChecklistView = {
  feature: "planner-search",
  pitch: null,
  has_product: true,
  sections: [],
  plans: { listed: 0, total: 0, written: 0, running: 0, done: 0 },
  untested: 0,
  metaplans: [],
  plan_rows: [],
  current: { kind: "step", number: 1 },
  steps: [],
  primary: null,
  secondary: [],
  items: [],
};
const kickoff: KickoffView = {
  session: "kickoff-session",
  target: "doc/product/planner-search/index.md",
  shape: {
    size: "feature",
    name: "planner-search",
    proposal: false,
    problem: "Find plans",
    decisions: [],
    questions: [],
    sections: [],
  },
};
const session = sessionViewSchema.parse({
  ...snapshotFixture.session,
  id: kickoff.session,
  state: "idle",
});

function Landing() {
  const view = useAtomValue(kickoffAtom);
  const current = useAtomValue(sessionAtom);
  useKickoffLanding(isActiveSession(current) && view?.session === current.id ? view : null);
  return null;
}

function mount(
  view: KickoffView | null = kickoff,
  checklists: ChecklistView[] = [],
  current: SessionView | null = session,
) {
  const store = createStore();
  store.set(kickoffAtom, view);
  store.set(snapshotAtom, { session: current, turns: [], queue: [], previous: null });
  store.set(projectAtom, { checklists, standalone: [], moves: [] });
  // Keep the hook mounted across navigation to exercise repeated snapshot updates.
  const router = createMemoryRouter([{ path: "*", element: createElement(Landing) }], {
    initialEntries: ["/new"],
  });
  const navigate = vi.spyOn(router, "navigate");
  const rendered = render(
    createElement(
      StrictMode,
      null,
      createElement(Provider, { store }, createElement(RouterProvider, { router })),
    ),
  );
  return { ...rendered, store, router, navigate };
}

function publish(
  store: ReturnType<typeof createStore>,
  checklists: ChecklistView[] = [],
  standalone: StandalonePlanView[] = [],
) {
  act(() => store.set(projectAtom, { checklists, standalone, moves: [] }));
}

beforeEach(resetKickoffLanding);
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

it.each([null, { ...kickoff, target: null }])(
  "waits when no confirmed target exists: %j",
  (view) => {
    const { store, navigate, router } = mount(view);
    publish(store, [feature]);
    expect(navigate).not.toHaveBeenCalled();
    expect(router.state.location.pathname).toBe("/new");
  },
);

it("lands on a feature only when its exact checklist appears, once per target", async () => {
  const { store, router, navigate } = mount();
  expect(navigate).not.toHaveBeenCalled();
  publish(store, [{ ...feature, feature: "planner-search-other" }]);
  expect(navigate).not.toHaveBeenCalled();
  publish(store, [feature]);
  await waitFor(() => expect(router.state.location.pathname).toBe("/f/planner-search"));
  expect(router.state.historyAction).toBe("REPLACE");
  expect(navigate).toHaveBeenCalledOnce();
  expect(navigate.mock.calls[0]?.[1]).toMatchObject({ replace: true });
  publish(store, [{ ...feature }]);
  act(() => store.set(kickoffAtom, { ...kickoff, shape: { ...kickoff.shape } }));
  act(() => store.set(kickoffAtom, null));
  act(() => store.set(kickoffAtom, kickoff));
  expect(navigate).toHaveBeenCalledOnce();
});

it.each([
  null,
  { ...session, state: "ended" as const },
  { ...session, state: "failed" as const },
  { ...session, id: "other-session" },
])("ignores a stale target when the caller has no matching active session: %j", (current) => {
  const { router, navigate } = mount(kickoff, [feature], current);
  expect(router.state.location.pathname).toBe("/new");
  expect(navigate).not.toHaveBeenCalled();
});

it("lands once for an active kickoff when its document appears, even after remount", async () => {
  const { store, router, navigate, unmount } = mount();
  publish(store, [feature]);
  await waitFor(() => expect(router.state.location.pathname).toBe("/f/planner-search"));
  expect(navigate).toHaveBeenCalledOnce();
  unmount();

  const remounted = mount(kickoff, [feature]);
  expect(remounted.router.state.location.pathname).toBe("/new");
  expect(remounted.navigate).not.toHaveBeenCalled();
});

it("allows another kickoff session to land on the same target", async () => {
  const first = mount(kickoff, [feature]);
  await waitFor(() => expect(first.router.state.location.pathname).toBe("/f/planner-search"));
  first.unmount();

  const next = mount({ ...kickoff, session: "next-session" }, [feature], {
    ...session,
    id: "next-session",
  });
  await waitFor(() => expect(next.router.state.location.pathname).toBe("/f/planner-search"));
  expect(next.navigate).toHaveBeenCalledOnce();
});

it("uses the confirmed feature target rather than an evolving shape name", async () => {
  const { store, router } = mount({ ...kickoff, shape: { ...kickoff.shape, name: "other" } });
  publish(store, [feature]);
  await waitFor(() => expect(router.state.location.pathname).toBe("/f/planner-search"));
});

it("lands when confirmation arrives after the feature snapshot", async () => {
  const { store, router, navigate } = mount({ ...kickoff, target: null });
  publish(store, [feature]);
  expect(navigate).not.toHaveBeenCalled();
  act(() => store.set(kickoffAtom, kickoff));
  await waitFor(() => expect(router.state.location.pathname).toBe("/f/planner-search"));
  expect(navigate).toHaveBeenCalledOnce();
});

it("encodes the feature route segment", async () => {
  const name = "search & notes";
  const { store, router } = mount({ ...kickoff, target: `doc/product/${name}/index.md` });
  publish(store, [{ ...feature, feature: name }]);
  await waitFor(() =>
    expect(router.state.location.pathname).toBe(`/f/${encodeURIComponent(name)}`),
  );
});

it.each([
  "doc/plans/PLAN-search.md",
  "doc/plans/IN_PROGRESS-PLAN-search.md",
  "doc/plans/DONE-PLAN-search.md",
  "doc/plans/archive/RETIRED-PLAN-search.md",
])("lands on the written standalone plan at %s exactly once", async (path) => {
  const view: KickoffView = {
    ...kickoff,
    target: "doc/plans/PLAN-search.md",
    shape: { ...kickoff.shape, size: "plan" },
  };
  const { store, router, navigate } = mount(view);
  const plan: StandalonePlanView = {
    slug: "search",
    path,
    title: "Search",
    state: "written",
    last_change: null,
  };
  publish(store, [], [{ ...plan, path: "doc/plans/PLAN-other-search.md" }]);
  expect(navigate).not.toHaveBeenCalled();
  publish(store, [], [plan]);
  const expected = `/files/${path.slice(4)}`;
  await waitFor(() => expect(router.state.location.pathname).toBe(expected));
  expect(router.state.historyAction).toBe("REPLACE");
  publish(store, [], [{ ...plan }]);
  act(() => store.set(kickoffAtom, { ...view }));
  expect(navigate).toHaveBeenCalledOnce();
});

it("recognizes a lifecycle prefix on the confirmed target", async () => {
  const path = "doc/plans/DONE-PLAN-search.md";
  const { store, router } = mount({
    ...kickoff,
    target: path,
    shape: { ...kickoff.shape, size: "plan" },
  });
  publish(store, [], [{ slug: "search", path, title: "Search", state: "done", last_change: null }]);
  await waitFor(() =>
    expect(router.state.location.pathname).toBe("/files/plans/DONE-PLAN-search.md"),
  );
});

it("encodes standalone plan path segments without introducing a query or hash", async () => {
  const path = "doc/plans/PLAN-search & notes?#%.md";
  const { store, router } = mount({
    ...kickoff,
    target: path,
    shape: { ...kickoff.shape, size: "plan" },
  });
  publish(
    store,
    [],
    [{ slug: "search", path, title: "Search", state: "written", last_change: null }],
  );

  await waitFor(() =>
    expect(router.state.location.pathname).toBe(
      "/files/plans/PLAN-search%20%26%20notes%3F%23%25.md",
    ),
  );
  expect(router.state.location.search).toBe("");
  expect(router.state.location.hash).toBe("");
});

it("allows one landing for each new confirmed target", async () => {
  const { store, router, navigate } = mount();
  publish(store, [feature]);
  await waitFor(() => expect(router.state.location.pathname).toBe("/f/planner-search"));
  act(() => store.set(kickoffAtom, { ...kickoff, target: "doc/product/second/index.md" }));
  expect(navigate).toHaveBeenCalledOnce();
  publish(store, [feature, { ...feature, feature: "second" }]);
  await waitFor(() => expect(router.state.location.pathname).toBe("/f/second"));
  expect(navigate).toHaveBeenCalledTimes(2);
  act(() => store.set(kickoffAtom, kickoff));
  expect(navigate).toHaveBeenCalledTimes(2);
});

it.each([
  { ...kickoff, target: "doc/product/planner-search/section_01.md" },
  { ...kickoff, target: "doc/plans/PLAN-search.md" },
  { ...kickoff, shape: { ...kickoff.shape, size: null } },
])("ignores targets that do not match the chosen size: %j", (view) => {
  const { store, navigate } = mount(view);
  publish(
    store,
    [feature],
    [
      {
        slug: "search",
        path: "doc/plans/PLAN-search.md",
        title: "Search",
        state: "written",
        last_change: null,
      },
    ],
  );
  expect(navigate).not.toHaveBeenCalled();
});
