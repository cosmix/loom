import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import searchFixture from "@/plan/api/fixtures/files-search.json";
import {
  fileTreeSchema,
  searchResultsSchema,
  type FileTree,
  type SectionView,
} from "@/plan/api/schema-files";
import { searchDocs } from "@/plan/files/api";
import { Outline } from "@/plan/files/outline";
import { Tree } from "@/plan/files/tree";

vi.mock("@/plan/files/api", () => ({ searchDocs: vi.fn() }));

const fixture = (): FileTree => fileTreeSchema.parse(structuredClone(treeFixture));

function renderTree(tree: FileTree, entry = "/files") {
  return render(
    <Provider store={createStore()}>
      <MemoryRouter initialEntries={[entry]}>
        <Tree tree={tree} />
      </MemoryRouter>
    </Provider>,
  );
}

beforeEach(() => vi.mocked(searchDocs).mockReset());
afterEach(cleanup);

describe("tree", () => {
  it("collapses Archive and knowledge categories by default", () => {
    renderTree(fixture());
    expect(screen.getByRole("button", { name: "Archive" }).getAttribute("aria-expanded")).toBe(
      "false",
    );
    expect(screen.queryByText("Old brief")).toBeNull();
    expect(screen.getByText("Architecture")).toBeTruthy();
    expect(screen.queryByText("Overview")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Expand Architecture" }));
    expect(screen.getByText("Overview")).toBeTruthy();
    expect(screen.getByText("System overview")).toBeTruthy();
  });

  it("shows the file name on hover and a missing word on a gap", () => {
    renderTree(fixture());
    expect(screen.getByRole("link", { name: "Notes" }).getAttribute("title")).toBe("notes.md");
    const gap = screen.getByText("Section 2").closest("li") as HTMLElement;
    expect(within(gap).getByText("missing")).toBeTruthy();
    expect(screen.getByRole("link", { name: "Section 2" }).getAttribute("href")).toBe(
      "/files?gap=Section%202&feature=demo&target=doc%2Fproduct%2Fdemo%2Fsection_02.md",
    );
  });

  it("shows a running plan's state word and lock, and a plan's error count", () => {
    const tree = fixture();
    tree.plans[0] = { ...tree.plans[0], state: "running", locked: true, errors: 2 };
    renderTree(tree);
    const plan = screen.getByText("File mode").closest("li") as HTMLElement;
    expect(within(plan).getByText("running")).toBeTruthy();
    expect(within(plan).getByLabelText("Locked")).toBeTruthy();
    expect(within(plan).getByText("2 errors")).toBeTruthy();
    expect(within(plan).getByText(/2026-10-17/)).toBeTruthy();
  });

  it("tones the state and error glyphs, never their words", () => {
    const tree = fixture();
    tree.plans[0] = { ...tree.plans[0], state: "running", errors: 2 };
    renderTree(tree);
    const plan = screen.getByText("File mode").closest("li") as HTMLElement;
    const state = within(plan).getByText("running");
    const errors = within(plan).getByText("2 errors");
    expect(state.className).not.toContain("tone");
    expect(state.querySelector("svg")?.getAttribute("class")).toContain("tone-executing");
    expect(errors.className).not.toContain("tone");
    expect(errors.querySelector("svg")?.getAttribute("class")).toContain("tone-blocked");
  });

  it("filters by title and file name as you type", () => {
    renderTree(fixture());
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "notes.md" } });
    expect(screen.getByText("Notes")).toBeTruthy();
    expect(screen.queryByText("Vision")).toBeNull();
    expect(screen.queryByText("File mode")).toBeNull();
  });

  it("searches on Enter with the chosen scope and renders snippets as text", async () => {
    const results = searchResultsSchema.parse(structuredClone(searchFixture));
    results.hits[0].snippet = "A <b>useful</b> purpose";
    results.truncated = true;
    vi.mocked(searchDocs).mockResolvedValue({ ok: true, value: results });
    renderTree(fixture());
    const box = screen.getByRole("searchbox");
    fireEvent.change(box, { target: { value: "  purpose " } });
    fireEvent.click(screen.getByRole("button", { name: "Plans" }));
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(screen.getByRole("list", { name: "Search results" })).toBeTruthy());
    expect(searchDocs).toHaveBeenCalledWith("purpose", "plans");
    const hit = within(screen.getByRole("list", { name: "Search results" })).getByRole("button");
    expect(hit.textContent).toContain("<b>useful</b>");
    expect(hit.querySelector("b")).toBeNull();
    expect(hit.querySelector("mark")?.textContent).toBe("purpose");
    expect(screen.getByText("50 results shown; refine the search")).toBeTruthy();
    fireEvent.keyDown(box, { key: "Escape" });
    expect(screen.getByText("Vision")).toBeTruthy();
    expect((box as HTMLInputElement).value).toBe("");
  });

  it("searches again with the new scope when it changes while results show", async () => {
    const results = searchResultsSchema.parse(structuredClone(searchFixture));
    vi.mocked(searchDocs).mockResolvedValue({ ok: true, value: results });
    renderTree(fixture());
    const box = screen.getByRole("searchbox");
    fireEvent.change(box, { target: { value: "purpose" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await waitFor(() => expect(screen.getByRole("list", { name: "Search results" })).toBeTruthy());
    expect(searchDocs).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Knowledge" }));
    await waitFor(() => expect(searchDocs).toHaveBeenCalledTimes(2));
    expect(searchDocs).toHaveBeenLastCalledWith("purpose", "knowledge");
  });

  it("does not search on a scope change while only filtering", () => {
    renderTree(fixture());
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "notes" } });
    fireEvent.click(screen.getByRole("button", { name: "Plans" }));
    expect(searchDocs).not.toHaveBeenCalled();
  });

  it("opens the group holding the open document and lets it collapse", () => {
    const tree = fixture();
    render(
      <Provider store={createStore()}>
        <MemoryRouter>
          <Tree tree={tree} open="doc/plans/archive/briefs/old.md" />
        </MemoryRouter>
      </Provider>,
    );
    const archive = screen.getByRole("button", { name: "Archive" });
    expect(archive.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(archive);
    expect(archive.getAttribute("aria-expanded")).toBe("false");
  });

  it("focuses the search box on /", () => {
    renderTree(fixture());
    fireEvent.keyDown(window, { key: "/" });
    expect(document.activeElement).toBe(screen.getByRole("searchbox"));
  });
});

describe("outline", () => {
  it("nests ### headings under the ## before them", () => {
    const sections: SectionView[] = [
      { level: 1, title: "Top", anchor: "top", start_line: 1, end_line: 2 },
      { level: 2, title: "One", anchor: "one", start_line: 3, end_line: 6 },
      { level: 3, title: "One.a", anchor: "one-a", start_line: 5, end_line: 6 },
      { level: 2, title: "Two", anchor: "two", start_line: 7, end_line: 8 },
    ];
    render(
      <MemoryRouter>
        <Outline path="doc/notes.md" sections={sections} />
      </MemoryRouter>,
    );
    expect(screen.queryByText("Top")).toBeNull();
    const one = screen.getByRole("link", { name: "One" }).closest("li") as HTMLElement;
    expect(within(one).getByRole("link", { name: "One.a" }).getAttribute("href")).toBe(
      "/files/notes.md#one-a",
    );
    expect(
      within(screen.getByRole("link", { name: "Two" }).closest("li") as HTMLElement).queryByText(
        "One.a",
      ),
    ).toBeNull();
  });
});
