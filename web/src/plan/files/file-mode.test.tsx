import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter, useLocation, type InitialEntry } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { checklistViewSchema } from "@/plan/api/schema";
import {
  docViewSchema,
  fileTreeSchema,
  type DocView,
  type FileTree,
} from "@/plan/api/schema-files";
import { fetchDoc, fetchTree } from "@/plan/files/api";
import { docChangedAtom, treeAtom } from "@/plan/files/atoms";
import { FileMode } from "@/plan/files/file-mode";
import { checklistAtom } from "@/plan/state/atoms";

vi.mock("@/plan/files/api", () => ({
  fetchDoc: vi.fn(),
  fetchTree: vi.fn(),
  searchDocs: vi.fn(),
  useFileSocket: () => {},
}));
vi.mock("@/plan/markdown", () => ({
  Markdown: (props: {
    source: string;
    marks?: string[];
    sections: { anchor: string; title: string }[];
  }) => (
    <div data-testid="markdown" data-marks={props.marks?.join(",")}>
      {props.sections.map((s) => (
        <h2 key={s.anchor} id={s.anchor}>
          {s.title}
        </h2>
      ))}
      <p>{props.source}</p>
    </div>
  ),
}));
vi.mock("@/plan/files/plan/plan-view", () => ({
  PlanView: ({ doc }: { doc: DocView }) => <div data-testid="plan-view">{doc.plan?.slug}</div>,
}));

const scrolled = vi.fn();
const DOC_PATH = "doc/product/section_01.md";
const doc = (patch: Partial<DocView> = {}): DocView =>
  docViewSchema.parse({ ...structuredClone(docFixture), ...patch });
const planDoc = (): DocView => docViewSchema.parse(structuredClone(planFixture));
const tree = (): FileTree => fileTreeSchema.parse(structuredClone(treeFixture));
const ok = (value: DocView) => ({ ok: true as const, value });
const failure = (status: number) => ({ ok: false as const, status, error: "e", message: "boom" });

function setWide(wide: boolean) {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: wide,
      media: query,
      addEventListener() {},
      removeEventListener() {},
    }),
  });
}

function Where() {
  const { pathname, search } = useLocation();
  return <output data-testid="where">{pathname + search}</output>;
}

function renderMode(entry: InitialEntry, fileTree: FileTree = tree(), store = createStore()) {
  store.set(treeAtom, fileTree);
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[entry]}>
        <FileMode />
        <Where />
      </MemoryRouter>
    </Provider>,
  );
  return store;
}

function renamedTree(): FileTree {
  const moved = tree();
  moved.plans[0] = { ...moved.plans[0], path: "doc/plans/IN_PROGRESS-PLAN-file-mode.md" };
  return moved;
}

beforeEach(() => {
  setWide(true);
  scrolled.mockReset();
  Element.prototype.scrollIntoView = scrolled;
  vi.mocked(fetchTree).mockReset();
  vi.mocked(fetchDoc).mockReset();
});
afterEach(cleanup);

describe("file mode layout", () => {
  it("shows the tree and a hint when no document is chosen", () => {
    renderMode("/files");
    expect(screen.getByRole("navigation", { name: "Documents" })).toBeTruthy();
    expect(screen.getByText("Choose a document from the tree.")).toBeTruthy();
  });

  it("turns the tree into a drawer below 900px", () => {
    setWide(false);
    vi.mocked(fetchDoc).mockResolvedValue(ok(doc()));
    renderMode(`/files/product/section_01.md`);
    expect(screen.queryByRole("navigation", { name: "Documents" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Documents" }));
    expect(screen.getByRole("dialog", { name: "Documents" })).toBeTruthy();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Documents" })).toBeNull();
  });

  it("opening a gap shows its checklist item and the offer", () => {
    const store = createStore();
    const checklist = checklistViewSchema.parse(structuredClone(checklistFixture));
    const item = { ...checklist.items[1], text: "Section 2" };
    store.set(checklistAtom, { ...checklist, items: [item] });
    renderMode("/files?gap=Section%202", tree(), store);
    expect(screen.getByRole("heading", { name: "Section 2" })).toBeTruthy();
    expect(screen.getByText(item.offer?.label ?? "")).toBeTruthy();
  });
});

describe("reading column", () => {
  it("renders a document and its lock notice, and scrolls to the anchor", async () => {
    const notice = "This plan is done. Record changes in a new plan.";
    vi.mocked(fetchDoc).mockResolvedValue(
      ok(doc({ lock: { reason: "done", notice, plan: null } })),
    );
    renderMode("/files/product/section_01.md#purpose");
    await screen.findByTestId("markdown");
    expect(fetchDoc).toHaveBeenCalledWith(DOC_PATH);
    expect(screen.getByText(notice)).toBeTruthy();
    expect(scrolled.mock.contexts.some((el) => (el as Element).id === "purpose")).toBe(true);
  });

  it("notes a missing anchor and renders plans with the plan view", async () => {
    vi.mocked(fetchDoc).mockResolvedValue(ok(planDoc()));
    renderMode("/files/plans/PLAN-file-mode.md#gone");
    await screen.findByTestId("plan-view");
    expect(screen.getByText("Section #gone is not in this document any more")).toBeTruthy();
  });

  it("passes search marks from the location state to the renderer", async () => {
    vi.mocked(fetchDoc).mockResolvedValue(ok(doc()));
    renderMode({ pathname: "/files/product/section_01.md", state: { marks: ["purpose"] } });
    expect((await screen.findByTestId("markdown")).getAttribute("data-marks")).toBe("purpose");
  });

  it("shows not_found with a link to the renamed plan on a 404", async () => {
    vi.mocked(fetchDoc).mockResolvedValue(failure(404));
    renderMode("/files/plans/PLAN-file-mode.md", renamedTree());
    await screen.findByText("This document no longer exists");
    expect(screen.getByRole("button", { name: "Back to the tree" })).toBeTruthy();
    expect(screen.getByRole("link", { name: /File mode is now at/ }).getAttribute("href")).toBe(
      "/files/plans/IN_PROGRESS-PLAN-file-mode.md",
    );
  });

  it("follows a renamed plan when the tree moves it", async () => {
    vi.mocked(fetchDoc).mockResolvedValue(ok(planDoc()));
    const store = renderMode("/files/plans/PLAN-file-mode.md");
    await screen.findByTestId("plan-view");
    act(() => store.set(treeAtom, renamedTree()));
    await waitFor(() =>
      expect(screen.getByTestId("where").textContent).toBe(
        "/files/plans/IN_PROGRESS-PLAN-file-mode.md",
      ),
    );
  });

  it("offers Retry after a failed fetch", async () => {
    vi.mocked(fetchDoc).mockResolvedValueOnce(failure(500)).mockResolvedValue(ok(doc()));
    renderMode("/files/product/section_01.md");
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    await screen.findByTestId("markdown");
  });

  it("renders a truncated file as monospace text under the size notice", async () => {
    const big = doc({ truncated: true, too_large: true, size: 3 * 1048576, text: "# raw heading" });
    vi.mocked(fetchDoc).mockResolvedValue(ok(big));
    renderMode("/files/product/section_01.md");
    await screen.findByText(/This file is 3.0 MiB; file mode shows the first 1 MiB/);
    expect(screen.queryByTestId("markdown")).toBeNull();
    expect(screen.getByText("# raw heading").tagName).toBe("PRE");
  });

  it("refetches a stale document in place, keeps the heading, and links the change", async () => {
    const sections = [
      { level: 2, title: "A", anchor: "a", start_line: 1, end_line: 2 },
      { level: 2, title: "B", anchor: "b", start_line: 3, end_line: 4 },
    ];
    const first = doc({ text: "old", hash: "h1", sections });
    vi.mocked(fetchDoc)
      .mockResolvedValueOnce(ok(first))
      .mockResolvedValue(ok({ ...first, text: "new", hash: "h2" }));
    const store = renderMode("/files/product/section_01.md");
    await screen.findByText("old");
    scrolled.mockReset();
    act(() => store.set(docChangedAtom, { [DOC_PATH]: "h2" }));
    await screen.findByText("new");
    const link = screen.getByRole("link", { name: "changed just now" });
    expect(link.getAttribute("href")).toBe("/files/product/section_01.md?diff=open");
    expect(fetchDoc).toHaveBeenCalledTimes(2);
    await waitFor(() =>
      expect(scrolled.mock.contexts.some((el) => (el as Element).id === "b")).toBe(true),
    );
  });
});
