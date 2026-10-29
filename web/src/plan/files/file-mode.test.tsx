import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
vi.mock("@/plan/files/plan/plan-view", () => ({
  PlanView: ({ doc }: { doc: DocView }) => <div data-testid="plan-view">{doc.plan?.slug}</div>,
}));

const scrolled = vi.fn();
const DOC_PATH = "doc/product/demo/section_01.md";
const doc = (patch: Partial<DocView> = {}): DocView =>
  docViewSchema.parse({ ...structuredClone(docFixture), ...patch });
const planDoc = (): DocView =>
  docViewSchema.parse({ ...structuredClone(planFixture), path: treeFixture.plans[0]!.path });
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
    renderMode(`/files/product/demo/section_01.md`);
    expect(screen.queryByRole("navigation", { name: "Documents" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Documents" }));
    expect(screen.getByRole("dialog", { name: "Documents" })).toBeTruthy();
    fireEvent.keyDown(screen.getByRole("dialog", { name: "Documents" }), { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Documents" })).toBeNull();
  });

  it("opening a gap shows its checklist item and the offer", () => {
    const store = createStore();
    const checklist = checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0]));
    const item = { ...checklist.items[1], text: "Section 2" };
    store.set(checklistAtom, { ...checklist, items: [item] });
    renderMode("/files?gap=Section%202", tree(), store);
    expect(screen.getByRole("heading", { name: "Section 2" })).toBeTruthy();
    expect(screen.getByRole("navigation", { name: "Breadcrumb" }).textContent).toBe(
      "PlannerSection 2",
    );
    expect(screen.getByText("This document is missing.")).toBeTruthy();
    expect(screen.getByText(item.offer?.label ?? "")).toBeTruthy();
  });
});

describe("reading column", () => {
  it("renders a document and its lock notice, and scrolls to the anchor", async () => {
    const notice = "This plan is done. Record changes in a new plan.";
    vi.mocked(fetchDoc).mockResolvedValue(
      ok(doc({ lock: { reason: "done", notice, plan: null } })),
    );
    renderMode("/files/product/demo/section_01.md#purpose");
    await screen.findByText("Text");
    expect(fetchDoc).toHaveBeenCalledWith(DOC_PATH);
    expect(screen.getByText(notice)).toBeTruthy();
    await waitFor(() =>
      expect(scrolled.mock.contexts.some((el) => (el as Element).id === "purpose")).toBe(true),
    );
  });

  it("renders the title once when the first body H1 matches, and keeps later H1s", async () => {
    const title = "Reading title";
    vi.mocked(fetchDoc).mockResolvedValue(
      ok(
        doc({
          title,
          text: `# ${title}\n\nBody text.\n\n# Appendix\n`,
          sections: [],
        }),
      ),
    );
    renderMode("/files/product/demo/section_01.md");
    await screen.findByText("Body text.");
    expect(screen.getAllByRole("heading", { level: 1, name: title })).toHaveLength(1);
    expect(screen.getByRole("heading", { level: 1, name: "Appendix" })).toBeTruthy();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(2);
  });

  it("notes a missing anchor and renders plans with the plan view", async () => {
    vi.mocked(fetchDoc).mockResolvedValue(ok(planDoc()));
    renderMode("/files/plans/PLAN-file-mode.md#gone");
    await screen.findByTestId("plan-view");
    expect(screen.getByText("Section #gone is not in this document any more")).toBeTruthy();
  });

  it("passes search marks from the location state to the renderer", async () => {
    vi.mocked(fetchDoc).mockResolvedValue(ok(doc()));
    renderMode({ pathname: "/files/product/demo/section_01.md", state: { marks: ["purpose"] } });
    await screen.findByText("Text");
    expect(screen.getByText("Purpose", { selector: "mark" }).closest("h2")?.id).toBe("purpose");
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
    renderMode("/files/product/demo/section_01.md");
    fireEvent.click(await screen.findByRole("button", { name: "Retry" }));
    await screen.findByText("Text");
  });

  it("renders a truncated file as monospace text under the size notice", async () => {
    const big = doc({ truncated: true, too_large: true, size: 3 * 1048576, text: "# raw heading" });
    vi.mocked(fetchDoc).mockResolvedValue(ok(big));
    renderMode("/files/product/demo/section_01.md");
    await screen.findByText(/This file is 3.0 MiB; file mode shows the first 1 MiB/);
    expect(document.querySelector("[data-doc-container] .md")).toBeNull();
    expect(screen.getByText("# raw heading").tagName).toBe("PRE");
  });

  it("refetches a stale document in place, keeps the heading, and links the change", async () => {
    const sections = [
      { level: 2, title: "A", anchor: "a", start_line: 1, end_line: 2 },
      { level: 2, title: "B", anchor: "b", start_line: 3, end_line: 4 },
    ];
    const first = doc({ text: "## A\nold\n## B\n", hash: "h1", sections });
    vi.mocked(fetchDoc)
      .mockResolvedValueOnce(ok(first))
      .mockResolvedValue(ok({ ...first, text: "## A\nnew\n## B\n", hash: "h2" }));
    const store = renderMode("/files/product/demo/section_01.md");
    await screen.findByText("old");
    scrolled.mockReset();
    act(() => store.set(docChangedAtom, { [DOC_PATH]: "h2" }));
    await screen.findByText("new");
    const link = screen.getByRole("link", { name: "changed just now" });
    expect(link.getAttribute("href")).toBe("/files/product/demo/section_01.md?diff=open");
    expect(fetchDoc).toHaveBeenCalledTimes(2);
    await waitFor(() =>
      expect(scrolled.mock.contexts.some((el) => (el as Element).id === "b")).toBe(true),
    );
  });
});

describe("tree drawer", () => {
  const openDrawer = () => {
    fireEvent.click(screen.getByRole("button", { name: "Documents" }));
    return screen.getByRole("dialog", { name: "Documents" });
  };

  beforeEach(() => {
    setWide(false);
    vi.mocked(fetchDoc).mockResolvedValue(ok(doc({ path: "doc/product/demo/index.md" })));
  });

  it("is a modal that takes focus, hides the page behind it and returns focus on Escape", async () => {
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    const opener = screen.getByRole("button", { name: "Documents" });
    expect(opener.getAttribute("aria-expanded")).toBe("false");
    const dialog = openDrawer();
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(opener.getAttribute("aria-expanded")).toBe("true");
    expect(opener.getAttribute("aria-controls")).toBe(dialog.id);
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(screen.queryByRole("region", { name: "Document" })).toBeNull();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(opener));
  });

  it("keeps Escape in the search box from closing it until the box is empty", async () => {
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    const dialog = openDrawer();
    const box = within(dialog).getByRole("searchbox") as HTMLInputElement;
    fireEvent.change(box, { target: { value: "vis" } });
    fireEvent.keyDown(box, { key: "Escape" });
    expect(box.value).toBe("");
    expect(screen.getByRole("dialog", { name: "Documents" })).toBeTruthy();
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes on a hash change in the same document", async () => {
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    fireEvent.click(within(openDrawer()).getByRole("link", { name: "Purpose" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByTestId("where").textContent).toBe("/files/product/demo/index.md");
  });

  it("moves focus to the reading column after a document is chosen", async () => {
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    fireEvent.click(within(openDrawer()).getByRole("link", { name: "Notes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() =>
      expect(document.activeElement).toBe(screen.getByRole("region", { name: "Document" })),
    );
  });
});

describe("a changed document that cannot be refetched", () => {
  const sections = [{ level: 2, title: "A", anchor: "a", start_line: 1, end_line: 2 }];
  const first = () => doc({ text: "old", hash: "h1", sections });

  it("keeps the earlier text under a notice whose Retry loads the new one", async () => {
    vi.mocked(fetchDoc)
      .mockResolvedValueOnce(ok(first()))
      .mockResolvedValueOnce(failure(500))
      .mockResolvedValue(ok({ ...first(), text: "new", hash: "h2" }));
    const store = renderMode("/files/product/demo/section_01.md");
    await screen.findByText("old");
    act(() => store.set(docChangedAtom, { [DOC_PATH]: "h2" }));
    const notice = await screen.findByRole("alert");
    expect(notice.textContent).toContain("could not be loaded (boom)");
    expect(screen.getByText("old")).toBeTruthy();
    fireEvent.click(within(notice).getByRole("button", { name: "Retry" }));
    await screen.findByText("new");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(fetchDoc).toHaveBeenCalledTimes(3);
  });

  it("refetches again when the server reports a newer change", async () => {
    vi.mocked(fetchDoc)
      .mockResolvedValueOnce(ok(first()))
      .mockResolvedValueOnce(failure(500))
      .mockResolvedValue(ok({ ...first(), text: "newest", hash: "h3" }));
    const store = renderMode("/files/product/demo/section_01.md");
    await screen.findByText("old");
    act(() => store.set(docChangedAtom, { [DOC_PATH]: "h2" }));
    await screen.findByRole("alert");
    act(() => store.set(docChangedAtom, { [DOC_PATH]: "h3" }));
    await screen.findByText("newest");
    expect(screen.queryByRole("alert")).toBeNull();
  });
});

// A matchMedia whose answer can change after mount, to resize the window under the page.
function resizableMedia(initiallyWide: boolean): (wide: boolean) => void {
  let current = initiallyWide;
  const listeners = new Set<() => void>();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      get matches() {
        return current;
      },
      media: query,
      addEventListener: (_type: string, listener: () => void) => listeners.add(listener),
      removeEventListener: (_type: string, listener: () => void) => listeners.delete(listener),
    }),
  });
  return (wide) => {
    current = wide;
    act(() => listeners.forEach((listener) => listener()));
  };
}

describe("tree drawer on a phone", () => {
  const openDrawer = () => {
    fireEvent.click(screen.getByRole("button", { name: "Documents" }));
    return screen.getByRole("dialog", { name: "Documents" });
  };

  beforeEach(() => {
    setWide(false);
    vi.mocked(fetchDoc).mockResolvedValue(ok(doc({ path: "doc/product/demo/index.md" })));
  });

  it("closes when the open document's own entry is tapped", async () => {
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    const current = openDrawer().querySelector<HTMLElement>("a[aria-current='page']");
    expect(current).not.toBeNull();
    fireEvent.click(current!);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByTestId("where").textContent).toBe("/files/product/demo/index.md");
  });

  it("does not come back open after the window was wide", async () => {
    const resize = resizableMedia(false);
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    openDrawer();
    resize(true);
    expect(screen.getByRole("navigation", { name: "Documents" })).toBeTruthy();
    resize(false);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByRole("button", { name: "Documents" }).getAttribute("aria-expanded")).toBe(
      "false",
    );
  });

  it("focuses the sheet, not the search box, so no keyboard rises", async () => {
    renderMode("/files/product/demo/index.md");
    await screen.findByText("Text");
    const dialog = openDrawer();
    const box = within(dialog).getByRole("searchbox");
    expect(document.activeElement).toBe(dialog);
    expect(document.activeElement).not.toBe(box);
  });
});
