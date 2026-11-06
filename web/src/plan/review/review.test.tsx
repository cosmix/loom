import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider, createStore } from "jotai";
import type { ReactNode } from "react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import serverFixture from "@/plan/api/fixtures/server-info.json";
import { featureOfKey } from "@/plan/api/page-key";
import { serverInfoSchema } from "@/plan/api/schema";
import { conceptPageSchema } from "@/plan/api/schema-concept";
import { treeEntrySchema } from "@/plan/api/schema-files";
import { storageKey } from "@/plan/chat/storage";
import { KeyChipExtras } from "@/plan/concept/key-chip-extras";
import { PageHeading } from "@/plan/concept/page-heading";
import { TreeEntryRow } from "@/plan/files/tree-entry";
import { TreeEntryExtras } from "@/plan/files/tree-entry-extras";
import { PLANNER_SHORTCUT_GROUPS } from "@/plan/help/shortcuts";
import { planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";

import type { BuiltResponse } from "./api";
import { CHANGES_PATH, ChangedSinceControl } from "./changed-since";
import { ChangesPage } from "./changes-page";
import { checkpointAtom, type Checkpoint } from "./checkpoint";
import { hashesAtom, openQuestionsAtom, useReviewLive } from "./live";

vi.mock("@/plan/diff/diff-view", () => ({
  DiffView: ({ before, after }: { before: string | null; after: string | null }) => (
    <div role="group" aria-label="Loaded diff">
      {JSON.stringify({ before, after })}
    </div>
  ),
}));

const page = conceptPageSchema.parse(summaryFixture);
const path = page.path!;
const chip = page.key_chips[0]!;
const entry = treeEntrySchema.parse({
  ...treeFixture.product[0]!.children[0],
  path,
  kind: "section",
});
const server = serverInfoSchema.parse(serverFixture);
const checkpoint: Checkpoint = { commit: "c1", hashes: { [path]: "old" } };
const current = { head: "c2", docs: { [path]: "new" } };
const builtUrl = "/api/built?section=product%2Fdemo%2Fsection_01.md";
const changedAfter = {
  kind: "changed_after",
  slug: "feat-review",
  built: "2026-10-17",
  base_commit: "base",
} satisfies BuiltResponse["notes"][number];
let project = 0;
let replies: Record<string, unknown>;
let refusal: string | null;
let fetchMock: ReturnType<typeof vi.fn>;

beforeEach(() => {
  localStorage.clear();
  refusal = null;
  replies = {
    "/api/hashes": current,
    "/api/open-questions": { counts: { [path]: 3 } },
    "/api/compare?commit=c1": { commit: "c1", available: true, docs: [] },
    "/api/ref?name=main": { commit: "base" },
    "/api/compare?commit=base": {
      commit: "base",
      available: true,
      docs: [{ path, change: "changed" }],
    },
    [builtUrl]: { notes: [] },
    [`/api/at-commit?commit=base&path=${encodeURIComponent(path)}`]: {
      path,
      text: "Before the build",
    },
    "/api/doc?path=product%2Fdemo%2Fsection_01.md": {
      ...docFixture,
      path,
      text: "Current section",
      hash: "new",
    },
  };
  fetchMock = vi.fn(async (input: string) => {
    if (input === "/api/ref?name=main" && refusal !== null) {
      return new Response(JSON.stringify({ error: "refused", message: refusal }), { status: 422 });
    }
    const body = replies[input];
    return new Response(JSON.stringify(body ?? { error: "not_found", message: "Not found" }), {
      status: body === undefined ? 404 : 200,
    });
  });
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

function Live() {
  useReviewLive();
  return null;
}

function mount(
  children: ReactNode = <ChangedSinceControl />,
  options: {
    checkpoint?: Checkpoint | null;
    live?: boolean;
  } = {},
) {
  const store = createStore();
  const projectKey = `review-${++project}`;
  store.set(serverInfoAtom, { ...server, project_key: projectKey });
  if (options.checkpoint !== null) store.set(checkpointAtom, options.checkpoint ?? checkpoint);
  if (!options.live) store.set(hashesAtom, current);
  store.set(openQuestionsAtom, { [path]: 3 });
  const listeners = new Set<() => void>();
  store.set(planSocketAtom, {
    send: vi.fn(() => true),
    subscribe: vi.fn(() => () => {}),
    dispose: vi.fn(),
    onResync: (handler: () => void) => {
      listeners.add(handler);
      return () => {
        listeners.delete(handler);
      };
    },
  });
  const view = render(
    <Provider store={store}>
      <MemoryRouter>
        {options.live && <Live />}
        {children}
        <Routes>
          <Route path={CHANGES_PATH} element={<ChangesPage />} />
          <Route path="*" element={null} />
        </Routes>
      </MemoryRouter>
    </Provider>,
  );
  return { store, projectKey, listeners, view };
}

async function openList() {
  fireEvent.click(await screen.findByRole("button", { name: /Changed since you looked/ }));
  await screen.findByRole("button", { name: `Show diff for ${path}` });
}

function compareRef() {
  fireEvent.change(screen.getByLabelText("Compare with ref"), { target: { value: "main" } });
  fireEvent.click(screen.getByRole("button", { name: "Compare" }));
}

it("first-visit checkpoint set silently", async () => {
  const { store, projectKey } = mount(undefined, { checkpoint: null, live: true });
  await waitFor(() =>
    expect(store.get(checkpointAtom)).toEqual({ commit: "c2", hashes: current.docs }),
  );
  expect(JSON.parse(localStorage.getItem(storageKey(projectKey, "checkpoint"))!)).toEqual({
    commit: "c2",
    hashes: current.docs,
  });
  expect(screen.queryByRole("button", { name: /Changed since you looked/ })).toBeNull();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("mark all seen", async () => {
  const { store, projectKey } = mount();
  await openList();
  fireEvent.click(screen.getByRole("button", { name: "Mark all seen" }));
  // The checkpoint is cut from a fresh `/api/hashes` reply, so it lands after the request.
  await waitFor(() =>
    expect(store.get(checkpointAtom)).toEqual({ commit: "c2", hashes: current.docs }),
  );
  expect(JSON.parse(localStorage.getItem(storageKey(projectKey, "checkpoint"))!)).toEqual({
    commit: "c2",
    hashes: current.docs,
  });
  expect(await screen.findByText("Nothing changed since you looked.")).toBeTruthy();
});

it("resolveRef refusal shown", async () => {
  refusal = "The ref does not name a known commit";
  const { store } = mount();
  await openList();
  compareRef();
  expect(await screen.findByText(refusal)).toBeTruthy();
  expect(store.get(checkpointAtom)).toEqual(checkpoint);
  expect(fetchMock.mock.calls.some(([url]) => url === "/api/compare?commit=base")).toBe(false);
});

it("count on summary header and tree entry", async () => {
  const { store } = mount(
    <>
      <PageHeading page={page} />
      <ul aria-label="Tree">
        <TreeEntryRow entry={entry} forceOpen={false} />
      </ul>
    </>,
  );
  expect(
    within(screen.getByRole("banner")).getByRole("link", { name: "3 open questions" }),
  ).toBeTruthy();
  expect(
    within(within(screen.getByRole("list", { name: "Tree" })).getByRole("listitem")).getByRole(
      "link",
      { name: "Open questions · 3" },
    ),
  ).toBeTruthy();
  act(() => store.set(openQuestionsAtom, { [path]: 0 }));
  expect(
    within(screen.getByRole("banner")).queryByRole("link", { name: /open question/i }),
  ).toBeNull();
  // A document with no open question left draws no count in the tree either.
  expect(screen.queryByRole("link", { name: /open question/i })).toBeNull();
  act(() => store.set(openQuestionsAtom, {}));
  expect(screen.queryByRole("link", { name: /open question/i })).toBeNull();
  await waitFor(() => expect(fetchMock).toHaveBeenCalledWith(builtUrl, expect.anything()));
});

it("count links to open-questions anchor with a decorative legend glyph", () => {
  mount(
    <>
      <PageHeading page={page} />
      <TreeEntryExtras entry={{ ...entry, path: "product/demo/section_01.md" }} />
    </>,
  );
  const anchor = "/files/product/demo/section_01.md#open-questions";
  const chip = within(screen.getByRole("banner")).getByRole("link", { name: "3 open questions" });
  // The summary page's chip opens the source inside the feature; the tree entry opens file mode.
  expect(chip.getAttribute("href")).toBe(
    "/f/demo/source/product/demo/section_01.md#open-questions",
  );
  const links = screen.getAllByRole("link", { name: "Open questions · 3" });
  expect(links).toHaveLength(1);
  for (const link of links) {
    expect(link.getAttribute("href")).toBe(anchor);
    const glyph = link.querySelector('[data-other-glyph="openQuestions"]');
    expect(glyph).not.toBeNull();
    expect(glyph?.getAttribute("aria-hidden")).toBe("true");
    expect(link.firstElementChild).toBe(glyph);
  }
});

it("dots on key chip and tree entry", () => {
  const { store } = mount(
    <>
      <KeyChipExtras chip={{ ...chip, doc: { ...chip.doc, path: "product/demo/section_01.md" } }} />
      <ul>
        <TreeEntryRow entry={entry} forceOpen={false} />
      </ul>
    </>,
  );
  expect(screen.getAllByRole("img", { name: "changed since you looked" })).toHaveLength(2);
  expect(
    within(screen.getByRole("listitem")).getByRole("img", { name: "changed since you looked" }),
  ).toBeTruthy();
  act(() => store.set(hashesAtom, { head: "c3", docs: { [path]: "old", "doc/other.md": "new" } }));
  expect(screen.queryByRole("img", { name: "changed since you looked" })).toBeNull();
});

it("marker links diff from base commit", async () => {
  replies[builtUrl] = { notes: [changedAfter] };
  const { store } = mount(<PageHeading page={page} />);
  const marker = await screen.findByRole("link", { name: "Changed after feat-review was built" });
  expect(marker.getAttribute("href")).toBe("/files/product/demo/section_01.md?base_commit=base");
  fireEvent.click(marker);
  expect((await screen.findByRole("group", { name: "Loaded diff" })).textContent).toBe(
    JSON.stringify({ before: "Before the build", after: "Current section" }),
  );
  expect(fetchMock).toHaveBeenCalledWith(
    `/api/at-commit?commit=base&path=${encodeURIComponent(path)}`,
    expect.anything(),
  );
  expect(fetchMock).toHaveBeenCalledWith(
    "/api/doc?path=product%2Fdemo%2Fsection_01.md",
    expect.anything(),
  );
  expect(store.get(checkpointAtom)).toEqual(checkpoint);
});

it("marker tooltip states limits", async () => {
  replies[builtUrl] = { notes: [changedAfter] };
  mount(<PageHeading page={page} />);
  const marker = await screen.findByRole("link", { name: "Changed after feat-review was built" });
  fireEvent.focus(marker);
  const tooltip = await screen.findByRole("tooltip");
  expect(tooltip.textContent).toMatch(
    /loom run again.*regenerates its review.*moves the build date later/,
  );
  expect(tooltip.textContent).toContain("A done plan without a review shows no marker");
});

it("running note", async () => {
  replies[builtUrl] = { notes: [{ kind: "running", slug: "feat-running" }] };
  mount(<PageHeading page={page} />);
  const note = await screen.findByRole("note");
  expect(note.textContent).toContain("This plan is running; changes here do not reach it");
  expect(note.textContent).toContain("feat-running");
  expect(within(note).queryByRole("link")).toBeNull();
});

it("help keys hold the Anywhere group", () => {
  expect(PLANNER_SHORTCUT_GROUPS.filter(({ title }) => title === "Anywhere")).toHaveLength(1);
  expect(PLANNER_SHORTCUT_GROUPS.find(({ title }) => title === "Anywhere")?.rows).toContainEqual([
    "c",
    "Focus the chat composer",
  ]);
});

it("help keys hold c, y and n", () => {
  expect(PLANNER_SHORTCUT_GROUPS.find(({ title }) => title === "Anywhere")?.rows).toEqual(
    expect.arrayContaining([
      ["c", "Focus the chat composer"],
      ["y", "Allow the pinned permission request"],
      ["n", "Deny it, with a note"],
    ]),
  );
});

it("checkpoint keyed by project key", async () => {
  const { store, projectKey } = mount();
  const otherKey = `${projectKey}-other`;
  const other = { commit: "c2", hashes: { ...current.docs } };
  localStorage.setItem(storageKey(otherKey, "checkpoint"), JSON.stringify(other));
  act(() => store.set(serverInfoAtom, { ...server, project_key: otherKey }));
  expect(store.get(checkpointAtom)).toEqual(other);
  expect(screen.queryByRole("button", { name: /Changed since you looked/ })).toBeNull();
  act(() => store.set(serverInfoAtom, { ...server, project_key: projectKey }));
  expect(await screen.findByRole("button", { name: /Changed since you looked · 1/ })).toBeTruthy();
  expect(JSON.parse(localStorage.getItem(storageKey(projectKey, "checkpoint"))!)).toEqual(
    checkpoint,
  );
});

it("resync refetches the compare list", async () => {
  const { store, listeners, view } = mount();
  await openList();
  compareRef();
  await screen.findByText("Comparing with", { exact: false });
  await screen.findByRole("button", { name: `Show diff for ${path}` });
  replies["/api/compare?commit=base"] = {
    commit: "base",
    available: true,
    docs: [{ path: "doc/new.md", change: "added" }],
  };
  act(() => listeners.forEach((handler) => handler()));
  expect(await screen.findByText("doc/new.md")).toBeTruthy();
  expect(screen.queryByRole("button", { name: `Show diff for ${path}` })).toBeNull();
  expect(fetchMock.mock.calls.filter(([url]) => url === "/api/compare?commit=base")).toHaveLength(
    2,
  );
  expect(store.get(checkpointAtom)).toEqual(checkpoint);
  view.unmount();
  expect(listeners.size).toBe(0);
});

it.each([
  "overview/demo/product",
  "overview/demo/engineering",
  "demo/engineering/feat-review",
  "plans/feat-review",
])("does not request build markers for %s", (key) => {
  mount(<PageHeading page={{ ...page, key, feature: featureOfKey(key) }} />);
  // The header reads its own source document for Edit and Discuss; nothing else is requested.
  const urls = fetchMock.mock.calls.map(([url]) => String(url));
  expect(urls.filter((url) => !url.startsWith("/api/doc?path="))).toEqual([]);
});

it("pathless summaries and folders have no review indicators", () => {
  const { view } = mount(
    <>
      <PageHeading page={{ ...conceptPageSchema.parse(overviewFixture), path: null }} />
      <TreeEntryExtras entry={{ ...entry, path: null }} />
    </>,
  );
  expect(within(view.container).queryByRole("link", { name: /Open questions/ })).toBeNull();
  expect(screen.queryByRole("img", { name: "changed since you looked" })).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
});
