import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { StrictMode } from "react";
import { createMemoryRouter, Link, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import type { ChecklistView, ProjectChecklistView, StandalonePlanView } from "@/plan/api/schema";
import { treeAtom } from "@/plan/files/atoms";
import { HomeRoute, resetHomeAutoOpen } from "@/plan/home/home";
import { authoringAtom, planSocketAtom, projectAtom, serverInfoAtom } from "@/plan/state/atoms";

const emptyProject: ProjectChecklistView = { checklists: [], standalone: [], moves: [] };
const standalone: StandalonePlanView = {
  slug: "fix-search",
  title: "Fix search",
  path: "doc/plans/PLAN-fix-search.md",
  state: "written",
};

beforeEach(() => {
  resetHomeAutoOpen();
  vi.stubGlobal("fetch", vi.fn().mockReturnValue(new Promise(() => {})));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function feature(name: string): ChecklistView {
  return {
    feature: name,
    pitch: null,
    current: { kind: "step", number: 1 },
    steps: [{ number: 1, name: "Vision", mark: "gap" }],
    primary: null,
    secondary: [],
    items: [],
  };
}

function Destination({ title }: { title: string }) {
  return (
    <>
      <h1>{title}</h1>
      <Link to="/">Home</Link>
    </>
  );
}

function mount(
  project: ProjectChecklistView | null = emptyProject,
  readOnly = false,
  initialEntry = "/",
  store = createStore(),
) {
  store.set(projectAtom, project);
  store.set(authoringAtom, { holder: null });
  store.set(serverInfoAtom, {
    project: "loom",
    project_key: "home-test",
    port: 8080,
    remote: false,
    stopping: false,
    you: { public_id: "reader", name: "Reader" },
    read_only: readOnly
      ? [{ what: "tmux", message: "tmux is unavailable", fix: "Install tmux" }]
      : [],
  });
  const router = createMemoryRouter(
    [
      { path: "/", element: <HomeRoute /> },
      { path: "/f/:feature", element: <Destination title="Feature screen" /> },
      { path: "/new", element: <Destination title="Kickoff conversation" /> },
    ],
    { initialEntries: [initialEntry] },
  );
  const view = render(
    <StrictMode>
      <Provider store={store}>
        <RouterProvider router={router} />
      </Provider>
    </StrictMode>,
  );
  return { store, router, ...view };
}

it("shows loading until project data arrives", () => {
  mount(null);

  expect(screen.getByRole("status", { name: "Loading your project" })).toBeTruthy();
  expect(screen.queryByRole("heading", { name: "Start something new" })).toBeNull();
  expect(screen.queryByText("No features yet.")).toBeNull();
  expect(fetch).not.toHaveBeenCalled();
});

it("auto-opens a lone feature once and lets the Home link return", async () => {
  const { router } = mount({ ...emptyProject, checklists: [feature("search")] });

  expect(await screen.findByRole("heading", { name: "Feature screen" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/f/search");
  expect(router.state.historyAction).toBe("REPLACE");

  fireEvent.click(screen.getByRole("link", { name: "Home" }));

  expect(await screen.findByRole("heading", { name: "Start something new" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/");
  expect(screen.getByRole("link", { name: "search" })).toBeTruthy();
});

it.each(["/new", "/f/search"])("keeps Home visible after first arriving from %s", async (entry) => {
  const { router } = mount({ ...emptyProject, checklists: [feature("search")] }, false, entry);

  fireEvent.click(screen.getByRole("link", { name: "Home" }));

  expect(await screen.findByRole("heading", { name: "Start something new" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/");

  fireEvent.click(screen.getByRole("link", { name: "search" }));
  await screen.findByRole("heading", { name: "Feature screen" });
  fireEvent.click(screen.getByRole("link", { name: "Home" }));

  expect(await screen.findByRole("heading", { name: "Start something new" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/");
});

it("consumes auto-open on navigation before returning to the initial Home entry", async () => {
  const { store, router } = mount(null);
  await act(async () => {
    await router.navigate("/new");
  });
  act(() => store.set(projectAtom, { ...emptyProject, checklists: [feature("search")] }));
  fireEvent.click(screen.getByRole("link", { name: "Home" }));
  await screen.findByRole("heading", { name: "Start something new" });
  fireEvent.click(screen.getByRole("link", { name: "search" }));
  await screen.findByRole("heading", { name: "Feature screen" });

  await act(async () => {
    await router.navigate(-3);
  });

  expect(screen.getByRole("heading", { name: "Start something new" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/");
  expect(router.state.location.key).toBe("default");
});

it("loads the file tree when Home mounts with a plan socket", async () => {
  const store = createStore();
  store.set(planSocketAtom, {
    send: vi.fn(() => true),
    subscribe: () => () => {},
    onResync: () => () => {},
    dispose: vi.fn(),
  });
  const fetchMock = vi
    .fn()
    .mockImplementation(() => Promise.resolve(new Response(JSON.stringify(treeFixture))));
  vi.stubGlobal("fetch", fetchMock);

  mount(emptyProject, false, "/", store);

  await waitFor(() => expect(store.get(treeAtom)).toEqual(treeFixture));
  expect(fetchMock).toHaveBeenCalledWith(
    "/api/tree",
    expect.objectContaining({ method: "GET", credentials: "same-origin" }),
  );
});

it("preserves auto-open while waiting for the first project message", async () => {
  const { store, router } = mount(null);

  act(() => store.set(projectAtom, { ...emptyProject, checklists: [feature("search")] }));

  expect(await screen.findByRole("heading", { name: "Feature screen" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/f/search");
});

it("encodes the feature name in the auto-open destination", async () => {
  const { router } = mount({ ...emptyProject, checklists: [feature("search / café")] });

  expect(await screen.findByRole("heading", { name: "Feature screen" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/f/search%20%2F%20caf%C3%A9");
});

it("resets the auto-open guard for a fresh page load", async () => {
  const project = { ...emptyProject, checklists: [feature("search")] };
  const first = mount(project);
  await screen.findByRole("heading", { name: "Feature screen" });
  first.unmount();
  resetHomeAutoOpen();

  const second = mount(project);

  expect(await screen.findByRole("heading", { name: "Feature screen" })).toBeTruthy();
  expect(second.router.state.location.pathname).toBe("/f/search");
});

it("renders the invitation followed by feature cards and the standalone shelf", () => {
  mount({
    checklists: [feature("search"), feature("billing")],
    standalone: [standalone],
    moves: [],
  });

  const invitation = screen.getByRole("region", { name: "Start something new" });
  const cards = screen.getByRole("list", { name: "Features" });
  const shelf = screen.getByRole("region", { name: "Standalone plans" });
  expect(within(cards).getByRole("link", { name: "search" }).getAttribute("href")).toBe(
    "/f/search",
  );
  expect(within(cards).getByRole("link", { name: "billing" }).getAttribute("href")).toBe(
    "/f/billing",
  );
  expect(within(shelf).getByRole("link", { name: "Fix search fix-search written" })).toBeTruthy();
  expect(invitation.compareDocumentPosition(cards) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(cards.compareDocumentPosition(shelf) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.queryByText("No features yet.")).toBeNull();
});

it("one feature and a standalone plan opens the feature", async () => {
  const { router } = mount({
    checklists: [feature("search")],
    standalone: [standalone],
    moves: [],
  });

  expect(await screen.findByRole("heading", { name: "Feature screen" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/f/search");
  expect(router.state.historyAction).toBe("REPLACE");
});

it("navigating back to Home does not redirect", async () => {
  const { router } = mount({
    checklists: [feature("search")],
    standalone: [standalone],
    moves: [],
  });
  await screen.findByRole("heading", { name: "Feature screen" });

  fireEvent.click(screen.getByRole("link", { name: "Home" }));

  expect(await screen.findByRole("heading", { name: "Start something new" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/");
  expect(screen.getByRole("link", { name: "search" })).toBeTruthy();
  expect(screen.getByRole("region", { name: "Standalone plans" })).toBeTruthy();
});

it("does not auto-open when later project updates leave only one feature", () => {
  const { store, router } = mount({
    ...emptyProject,
    checklists: [feature("search"), feature("billing")],
  });

  act(() => store.set(projectAtom, { ...emptyProject, checklists: [feature("search")] }));

  expect(screen.getByRole("heading", { name: "Start something new" })).toBeTruthy();
  expect(router.state.location.pathname).toBe("/");
});

it("shows the empty lead and accessible doodle as the whole empty screen", () => {
  mount();

  const invitation = screen.getByRole("region", { name: "Start something new" });
  expect(within(invitation).getByText("No features yet.")).toBeTruthy();
  expect(
    within(invitation).getByRole("img", {
      name: "A thread leaves a spool, loops through a thought and weaves into cloth on a loom",
    }),
  ).toBeTruthy();
  expect(invitation.hasAttribute("data-doodle-host")).toBe(true);
  expect(within(invitation).getByText("Nothing is written until you say so.")).toBeTruthy();
  expect(screen.queryByRole("list", { name: "Features" })).toBeNull();
  expect(screen.queryByRole("region", { name: "Standalone plans" })).toBeNull();
});

it("starts a conversation through the kickoff link", async () => {
  mount();
  const start = screen.getByRole("link", { name: "Start a conversation" });
  expect(start.getAttribute("href")).toBe("/new");

  fireEvent.click(start);

  expect(await screen.findByRole("heading", { name: "Kickoff conversation" })).toBeTruthy();
});

it("explains read-only authoring and offers no kickoff link", () => {
  const { router, container } = mount(emptyProject, true);

  const start = screen.getByRole("button", { name: "Start a conversation" });
  expect(start.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("loom plan is in read-only mode: tmux is unavailable")).toBeTruthy();
  expect(screen.queryByRole("link", { name: "Start a conversation" })).toBeNull();
  expect(container.querySelector('a[href="/new"]')).toBeNull();

  fireEvent.click(start);

  expect(router.state.location.pathname).toBe("/");
});

it("renders server authoring reasons with visible formatting controls", () => {
  const { store } = mount();

  act(() =>
    store.set(authoringAtom, {
      holder: { public_id: "other", name: "Alex\u202E", away_since: null, frees_at: null },
    }),
  );

  expect(screen.getByText("Alex⟨U+202E⟩ holds authoring")).toBeTruthy();
  expect(
    screen.getByRole("button", { name: "Start a conversation" }).getAttribute("aria-disabled"),
  ).toBe("true");
});
