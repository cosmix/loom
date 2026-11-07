import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { pendingAttachmentsAtom } from "@/plan/attachments";
import { draftAtom } from "./drafts";
import { TurnFooter } from "./turn-footer";
import {
  mount,
  path,
  requests,
  failures,
  diffs,
  resetNet,
  respond,
  sourcePaths,
  turn,
} from "./turn-footer-kit";

const { restoreButton, restoreAll } = vi.hoisted(() => ({
  restoreButton: vi.fn(),
  restoreAll: vi.fn(),
}));
vi.mock("./restore", () => ({
  RestoreButton: (props: unknown) => {
    restoreButton(props);
    return null;
  },
  RestoreAll: (props: unknown) => {
    restoreAll(props);
    return null;
  },
}));

beforeEach(() => {
  resetNet();
  localStorage.clear();
  vi.clearAllMocks();
  vi.stubGlobal("fetch", vi.fn(respond));
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("the footer shows neither duration nor tokens", () => {
  const { container } = mount(turn({ changed_files: [] }));
  expect(screen.queryByText("1m5s")).toBeNull();
  expect(container.textContent).not.toMatch(/tokens/);
  expect(screen.getByRole("group", { name: "Turn 2 changes" })).toBeTruthy();
});

it("rows and diff link", async () => {
  mount();
  expect(screen.getByText("Changed 1 file")).toBeTruthy();
  expect(screen.getByText("section_02.md").closest("[title]")?.getAttribute("title")).toBe(
    "product/demo/section_02.md",
  );
  expect(await screen.findByText("+1 −1")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Diff" }).getAttribute("href")).toContain("?diff=turn");
  expect(restoreButton).toHaveBeenCalledWith(
    expect.objectContaining({ path, sessionId: "session one" }),
  );
});

it("footer diff link carries the session/turn key and a doc-relative path", async () => {
  mount();
  const href = screen.getByRole("link", { name: "Diff" }).getAttribute("href")!;
  const url = new URL(href, "http://localhost");
  expect(url.pathname).toBe("/files/product/demo/section_02.md");
  expect(url.searchParams.get("turn")).toBe("session one/t2");
  await waitFor(() =>
    expect(
      requests.some((request) =>
        request.includes("turn=session%20one%2Ft2&path=product%2Fdemo%2Fsection_02.md"),
      ),
    ).toBe(true),
  );
});

it("running turn disables diff", () => {
  mount(turn({ open: true }));
  expect(screen.getByText("Changing 1 file")).toBeTruthy();
  expect(screen.queryByRole("link", { name: "Diff" })).toBeNull();
  expect(screen.getByRole("button", { name: "Diff" }).getAttribute("aria-disabled")).toBe("true");
  expect(screen.getByText("Available when the turn ends")).toBeTruthy();
  expect(restoreButton).not.toHaveBeenCalled();
});

it("by_tool false note", async () => {
  mount(turn({ changed_files: [{ path, before_hash: "before", by_tool: false }] }));
  expect(screen.getByText("changed during this turn")).toBeTruthy();
  await screen.findByText("+1 −1");
});

it("plans under a changed feature section grouped by state", async () => {
  mount();
  const written = await screen.findByRole("group", { name: "written plans" });
  expect(written.textContent).toContain("may need updating");
  expect(within(written).getByRole("link", { name: "Draft plan" }).getAttribute("href")).toBe(
    "/f/demo/engineering/draft",
  );
  expect(screen.getByRole("link", { name: "Active plan" }).getAttribute("href")).toBe(
    "/plans/active",
  );
  for (const state of ["running", "done"]) {
    const group = screen.getByRole("group", { name: `${state} plans` });
    expect(group.textContent).toContain("built from earlier text");
    expect(within(group).getByRole("button", { name: "Plan a follow-up" })).toBeTruthy();
  }
});

it("no files no list", () => {
  mount(turn({ changed_files: [] }));
  expect(screen.queryByRole("list")).toBeNull();
  expect(screen.queryByText(/Changed/)).toBeNull();
});

it("rebuilt rows states", () => {
  const value = turn({
    changed_files: [],
    open: true,
    summaries_rebuilding: ["demo/product/section_01", "plans/draft", "plans/y"],
  });
  const { container, rerender } = mount(value);
  expect(screen.getAllByText("Rebuilding")).toHaveLength(3);
  expect(screen.getByText("3 summaries rebuilding")).toBeTruthy();
  expect(container.querySelector(".aurora-roundel")).toBeNull();
  expect(screen.getByRole("link", { name: "demo/product/section_01" }).getAttribute("href")).toBe(
    "/f/demo/product/section_01",
  );
  expect(screen.getByRole("link", { name: "plans/draft" }).getAttribute("href")).toBe(
    "/f/demo/engineering/draft",
  );
  expect(screen.getByRole("link", { name: "plans/y" }).getAttribute("href")).toBe("/plans/y");
  rerender(
    <MemoryRouter>
      <TurnFooter turn={{ ...value, open: false }} sessionId="session one" />
    </MemoryRouter>,
  );
  expect(screen.getAllByText("left for the background rebuild")).toHaveLength(3);
  expect(container.querySelector(".aurora-roundel")).toBeNull();
});

it("check these plans fills composer and attaches section and plans", async () => {
  const { store } = mount();
  fireEvent.click(await screen.findByRole("button", { name: "Check these plans" }));
  await waitFor(() =>
    expect(store.get(draftAtom).text).toBe(
      "Keep until fill succeeds\n\nCheck these plans against the change to section_02.",
    ),
  );
  expect(store.get(pendingAttachmentsAtom)).toEqual(
    sourcePaths.slice(0, 2).map((name) => ({
      kind: "source",
      path: name,
      text: `Source of ${name.slice(4)}`,
      hash: `hash:${name.slice(4)}`,
      anchor: null,
      lines: null,
      stageId: null,
    })),
  );
});

it("plan a follow-up fill", async () => {
  const { store } = mount();
  const group = await screen.findByRole("group", { name: "running plans" });
  fireEvent.click(within(group).getByRole("button", { name: "Plan a follow-up" }));
  await waitFor(() =>
    expect(store.get(draftAtom).text).toBe(
      "Keep until fill succeeds\n\nPlan a follow-up for the change to section_02.",
    ),
  );
  expect(
    store
      .get(pendingAttachmentsAtom)
      .map((attachment) => attachment.kind === "source" && attachment.path),
  ).toEqual([path, sourcePaths[2]]);
});

it("fill into an empty draft adds no leading blank line", async () => {
  const { store } = mount();
  store.set(draftAtom, { text: "", attachments: [] });
  fireEvent.click(await screen.findByRole("button", { name: "Check these plans" }));
  await waitFor(() =>
    expect(store.get(draftAtom).text).toBe("Check these plans against the change to section_02."),
  );
});

it("viewer sees no fill actions", async () => {
  mount(turn(), false);
  await screen.findByRole("group", { name: "written plans" });
  expect(screen.queryByRole("button", { name: "Check these plans" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Plan a follow-up" })).toBeNull();
});

it("fill request failure shows the reason and keeps the draft", async () => {
  failures["/api/doc"] = "Source is unavailable";
  const { store } = mount();
  fireEvent.click(await screen.findByRole("button", { name: "Check these plans" }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Source is unavailable");
  expect(store.get(draftAtom).text).toBe("Keep until fill succeeds");
  expect(store.get(pendingAttachmentsAtom)).toEqual([]);
});

it("restore request failure shows the reason", async () => {
  failures["/api/restore"] = "Turn pins have expired";
  const { store } = mount();
  const { RestoreButton } = await vi.importActual<typeof import("./restore")>("./restore");
  render(
    <Provider store={store}>
      <MemoryRouter>
        <RestoreButton sessionId="session one" turn={turn()} path={path} />
      </MemoryRouter>
    </Provider>,
  );
  fireEvent.click(await screen.findByRole("button", { name: "Restore" }));
  expect(await screen.findByRole("alert")).toHaveProperty("textContent", "Turn pins have expired");
});

it("outside doc paths have a warning and no Diff link", async () => {
  const outside = "planning-\u202eomissions.md";
  mount(
    turn({
      changed_files: [...turn().changed_files, { path: outside, before_hash: "h", by_tool: true }],
    }),
  );
  const label = screen.getByText("planning-⟨U+202E⟩omissions.md");
  const row = label.closest("li")!;
  expect(within(row).getByText(/outside doc\//)).toBeTruthy();
  expect(within(row).queryByRole("link")).toBeNull();
  expect(screen.getAllByRole("link", { name: "Diff" })).toHaveLength(1);
  await screen.findByText("+1 −1");
});

it("restore all receives only modified files after diffs resolve", async () => {
  const paths = ["doc/a.md", "doc/b.md", "doc/new.md", "doc/deleted.md"];
  diffs["new.md"] = { before: null, after: "created" };
  diffs["deleted.md"] = { before: "deleted", after: null };
  mount(
    turn({
      changed_files: paths.map((name) => ({
        path: name,
        before_hash: name.endsWith("new.md") ? null : "h",
        by_tool: true,
      })),
    }),
  );
  await screen.findByText("deleted");
  expect(restoreAll).toHaveBeenLastCalledWith(
    expect.objectContaining({ paths: paths.slice(0, 2) }),
  );
  expect(screen.getByText("new")).toBeTruthy();
});

it("fetches plans once per turn and scopes them to the changed feature section", async () => {
  const sections = [path, "doc/product/other/section_02.md"];
  mount(
    turn({
      changed_files: sections.map((name) => ({ path: name, before_hash: "h", by_tool: true })),
    }),
  );
  await screen.findAllByText("+1 −1");
  await screen.findAllByRole("group", { name: "written plans" });
  const other = screen.getByTitle("product/other/section_02.md").closest("li")!;
  expect(within(other).queryByRole("group")).toBeNull();
  const count = (pathname: string) => requests.filter((r) => r.startsWith(pathname)).length;
  expect(count("/api/implemented-by")).toBe(1);
  expect(count("/api/diff/turn")).toBe(2);
  expect(restoreButton).toHaveBeenCalledWith(
    expect.objectContaining({ texts: expect.arrayContaining([expect.objectContaining({ path })]) }),
  );
});
