import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { pendingAttachmentsAtom } from "@/plan/attachments/atoms";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { DiffView } from "./diff-view";

const destroy = vi.fn();
const scrollToFirstChange = vi.fn();
const createDiffView = vi.fn<(parent: HTMLElement, options: unknown) => Record<string, unknown>>(
  () => ({ destroy, scrollToFirstChange }),
);
vi.mock("@/plan/editor/load", () => ({
  loadCodeMirror: () => Promise.resolve({ createDiffView }),
}));

function mockWidth(wide: boolean) {
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockImplementation((query: string) => ({
      matches: wide,
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    })),
  );
}

/** The diff opens on the rendered document; the CodeMirror view is built once Source is chosen. */
function showSource(): void {
  fireEvent.click(screen.getByRole("button", { name: "Source" }));
}

const ME = { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" };

beforeEach(() => {
  // Add to chat is offered only to the browser that holds authoring.
  store.set(serverInfoAtom, {
    project: "example",
    project_key: "example-1234",
    port: 7272,
    remote: false,
    read_only: [],
    stopping: false,
    you: ME,
  });
  store.set(youAtom, ME);
  store.set(authoringAtom, { holder: { ...ME, away_since: null, frees_at: null } });
  createDiffView.mockClear();
  destroy.mockClear();
  scrollToFirstChange.mockClear();
});
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  cleanup();
  vi.unstubAllGlobals();
});

describe("DiffView", () => {
  it("opens on the rendered document and builds the editor only for Source", async () => {
    mockWidth(true);
    render(
      <DiffView
        before={"# Title\n\nOld words.\n"}
        after={"# Title\n\nNew words.\n"}
        path="doc/x.md"
      />,
    );
    const rendered = screen.getByRole("button", { name: "Rendered" });
    const source = screen.getByRole("button", { name: "Source" });
    expect(rendered.getAttribute("aria-pressed")).toBe("true");
    expect(source.getAttribute("aria-pressed")).toBe("false");
    expect(document.querySelector('[data-diff-block="changed"]')).not.toBeNull();
    // A tick for the loader's promise, had the rendered view asked for it.
    await act(async () => {});
    expect(createDiffView).not.toHaveBeenCalled();

    fireEvent.click(source);
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(source.getAttribute("aria-pressed")).toBe("true");
    expect(rendered.getAttribute("aria-pressed")).toBe("false");
    expect(document.querySelector("[data-diff-block]")).toBeNull();

    fireEvent.click(rendered);
    expect(destroy).toHaveBeenCalledTimes(1);
    expect(document.querySelector('[data-diff-block="changed"]')).not.toBeNull();
  });

  it("renders side by side at 900px and wider", async () => {
    mockWidth(true);
    render(<DiffView before="a" after="b" path="doc/x.md" />);
    showSource();
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(createDiffView).toHaveBeenCalledWith(expect.anything(), {
      before: "a",
      after: "b",
      unified: false,
    });
    expect(screen.getByLabelText("Diff of doc/x.md").getAttribute("data-diff-layout")).toBe(
      "split",
    );
  });

  it("is unified below 900px", async () => {
    mockWidth(false);
    render(<DiffView before="a" after="b" path="doc/x.md" />);
    showSource();
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(createDiffView).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ unified: true }),
    );
    expect(screen.getByLabelText("Diff of doc/x.md").getAttribute("data-diff-layout")).toBe(
      "unified",
    );
  });

  it("diffs a created file against nothing and a deleted file to nothing", async () => {
    mockWidth(true);
    const created = render(<DiffView before={null} after="new" path="doc/x.md" />);
    expect(screen.getByText("New file")).toBeTruthy();
    showSource();
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(createDiffView).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ before: "", after: "new" }),
    );
    created.unmount();
    expect(destroy).toHaveBeenCalled();
    render(<DiffView before="old" after={null} path="doc/x.md" />);
    expect(screen.getByText("Deleted file")).toBeTruthy();
    showSource();
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(2));
    expect(createDiffView).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ before: "old", after: "" }),
    );
  });

  it("counts the blocks of each kind in its head", () => {
    mockWidth(true);
    const before = "Kept.\n\nReworded once.\n\nDropped.\n";
    const after = "Kept.\n\nReworded twice.\n\nFresh.\n\nFresher.\n";
    const view = render(<DiffView before={before} after={after} path="doc/x.md" />);
    const summary = () => view.container.querySelector(".diff-summary")?.textContent;
    // The dropped paragraph pairs with the first new one, so two blocks changed and one was added.
    expect(summary()).toBe("+1 added ±2 changed");
    view.rerender(<DiffView before={before} after={"Kept.\n"} path="doc/x.md" />);
    expect(summary()).toBe("−2 removed");
    view.rerender(<DiffView before={before} after={before} path="doc/x.md" />);
    expect(summary()).toBe("No changes");
  });

  it("scrolls the source view to its first change once, not again when it is rebuilt", async () => {
    mockWidth(true);
    const view = render(<DiffView before="a" after="b" path="doc/x.md" />);
    showSource();
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(scrollToFirstChange).toHaveBeenCalledTimes(1);
    view.rerender(<DiffView before="a" after="c" path="doc/x.md" />);
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(2));
    expect(scrollToFirstChange).toHaveBeenCalledTimes(1);
  });

  it("adds a selection from either side of the diff to the chat", async () => {
    mockWidth(true);
    // jsdom does not implement layout for ranges.
    Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
    createDiffView.mockImplementationOnce((parent) => {
      for (const side of ["before-side", "after-side"]) {
        const pane = document.createElement("div");
        pane.dataset.testid = side;
        pane.textContent = `${side} words`;
        parent.append(pane);
      }
      // Both panes belong to the diff handle; the line is the second of the document.
      return {
        destroy,
        scrollToFirstChange,
        rangeLines: (range: Range) =>
          parent.contains(range.startContainer) ? { start: 2, end: 2 } : null,
      };
    });
    render(
      <Provider store={store}>
        <DiffView before="a" after="b" path="doc/x.md" />
      </Provider>,
    );
    showSource();
    for (const side of ["before-side", "after-side"]) {
      const pane = await screen.findByTestId(side);
      const range = document.createRange();
      range.setStart(pane.firstChild as Node, 0);
      range.setEnd(pane.firstChild as Node, 4);
      const selection = window.getSelection() as Selection;
      selection.removeAllRanges();
      selection.addRange(range);
      act(() => void document.dispatchEvent(new Event("selectionchange")));
      expect(await screen.findByText("Add to chat")).toBeTruthy();
    }
  });

  it("offers nothing when the editor cannot attribute lines to the selection", async () => {
    mockWidth(true);
    Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
    createDiffView.mockImplementationOnce((parent) => {
      const pane = document.createElement("div");
      pane.dataset.testid = "pane";
      pane.textContent = "pane words";
      parent.append(pane);
      return { destroy, scrollToFirstChange };
    });
    render(
      <Provider store={store}>
        <DiffView before="a" after="b" path="doc/x.md" />
      </Provider>,
    );
    showSource();
    const pane = await screen.findByTestId("pane");
    const range = document.createRange();
    range.setStart(pane.firstChild as Node, 0);
    range.setEnd(pane.firstChild as Node, 4);
    const selection = window.getSelection() as Selection;
    selection.removeAllRanges();
    selection.addRange(range);
    act(() => void document.dispatchEvent(new Event("selectionchange")));
    expect(screen.queryByText("Add to chat")).toBeNull();
  });

  it("offers nothing for a selection in the rendered document", () => {
    mockWidth(true);
    Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
    render(
      <Provider store={store}>
        <DiffView before="Old words here." after="New words here." path="doc/x.md" />
      </Provider>,
    );
    const paragraph = screen.getByText("New words here.");
    const range = document.createRange();
    range.selectNodeContents(paragraph);
    const selection = window.getSelection() as Selection;
    selection.removeAllRanges();
    selection.addRange(range);
    act(() => void document.dispatchEvent(new Event("selectionchange")));
    expect(screen.queryByText("Add to chat")).toBeNull();
  });
});

describe("DiffView anchors", () => {
  it("gives a before-side selection no anchor and an after-side one its section", async () => {
    mockWidth(true);
    Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
    store.set(pendingAttachmentsAtom, []);
    createDiffView.mockImplementationOnce((parent) => {
      for (const side of ["before-side", "after-side"]) {
        const pane = document.createElement("div");
        pane.dataset.testid = side;
        pane.textContent = `${side} words`;
        parent.append(pane);
      }
      return {
        destroy,
        scrollToFirstChange,
        rangeLines: (range: Range) => ({
          start: 2,
          end: 2,
          side: parent.firstChild?.contains(range.startContainer) ? "before" : "after",
        }),
      };
    });
    const sections = [{ level: 2, title: "Intro", anchor: "intro", start_line: 1, end_line: 9 }];
    render(
      <Provider store={store}>
        <DiffView before="a" after="b" path="doc/x.md" sections={sections} />
      </Provider>,
    );
    showSource();
    for (const side of ["before-side", "after-side"]) {
      const pane = await screen.findByTestId(side);
      const range = document.createRange();
      range.setStart(pane.firstChild as Node, 0);
      range.setEnd(pane.firstChild as Node, 4);
      const selection = window.getSelection() as Selection;
      selection.removeAllRanges();
      selection.addRange(range);
      act(() => void document.dispatchEvent(new Event("selectionchange")));
      fireEvent.click(await screen.findByText("Add to chat"));
      await waitFor(() => expect(screen.queryByText("Add to chat")).toBeNull());
    }
    const anchors = store
      .get(pendingAttachmentsAtom)
      .map((draft) => (draft.kind === "source" ? draft.anchor : "summary"));
    expect(anchors).toEqual([null, "intro"]);
  });
});
