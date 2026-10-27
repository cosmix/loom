import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { pendingAttachmentsAtom } from "@/plan/attachments/atoms";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { DiffView } from "./diff-view";

const destroy = vi.fn();
const createDiffView = vi.fn<(parent: HTMLElement, options: unknown) => Record<string, unknown>>(
  () => ({ destroy }),
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
});
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  cleanup();
  vi.unstubAllGlobals();
});

describe("DiffView", () => {
  it("renders side by side at 900px and wider", async () => {
    mockWidth(true);
    render(<DiffView before="a" after="b" path="doc/x.md" />);
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(createDiffView).toHaveBeenCalledWith(expect.anything(), {
      before: "a",
      after: "b",
      unified: false,
    });
  });

  it("is unified below 900px", async () => {
    mockWidth(false);
    render(<DiffView before="a" after="b" path="doc/x.md" />);
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
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(1));
    expect(createDiffView).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ before: "", after: "new" }),
    );
    created.unmount();
    expect(destroy).toHaveBeenCalled();
    render(<DiffView before="old" after={null} path="doc/x.md" />);
    await waitFor(() => expect(createDiffView).toHaveBeenCalledTimes(2));
    expect(createDiffView).toHaveBeenLastCalledWith(
      expect.anything(),
      expect.objectContaining({ before: "old", after: "" }),
    );
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
        rangeLines: (range: Range) =>
          parent.contains(range.startContainer) ? { start: 2, end: 2 } : null,
      };
    });
    render(
      <Provider store={store}>
        <DiffView before="a" after="b" path="doc/x.md" />
      </Provider>,
    );
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
      return { destroy };
    });
    render(
      <Provider store={store}>
        <DiffView before="a" after="b" path="doc/x.md" />
      </Provider>,
    );
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
