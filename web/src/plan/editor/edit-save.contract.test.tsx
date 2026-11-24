import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { EditMode } from "@/plan/editor/edit-mode";
import { docChangedAtom } from "@/plan/files/atoms";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

// A stand-in for the lazy CodeMirror chunk: the source editor is a textarea whose
// input event drives onChange, so the test types and reads the editor text in the DOM.
const fakeCodeMirror = vi.hoisted(() => {
  function area(parent: HTMLElement, testId: string, value: string, readOnly: boolean) {
    const element = document.createElement("textarea");
    element.dataset.testid = testId;
    element.value = value;
    element.readOnly = readOnly;
    parent.append(element);
    return element;
  }
  function handle(element: HTMLTextAreaElement) {
    let mode: "preview" | "source" = "preview";
    return {
      destroy: () => element.remove(),
      getText: () => element.value,
      setText: (text: string) => {
        element.value = text;
      },
      searchOpen: () => false,
      setGutterMarks: () => {},
      scrollToLine: () => {},
      rangeLines: () => null,
      rangeText: () => null,
      mode: () => mode,
      setMode: (next: "preview" | "source") => {
        mode = next;
      },
      format: () => {},
    };
  }
  type Change = (text: string) => void;
  return {
    createSourceEditor(
      parent: HTMLElement,
      options: { doc: string; readOnly?: boolean; onChange: Change; onSave?: () => void },
    ) {
      const element = area(parent, "source-editor", options.doc, options.readOnly ?? false);
      element.addEventListener("input", () => options.onChange(element.value));
      return handle(element);
    },
    createMergeView(
      parent: HTMLElement,
      options: { left: string; right: string; onChange?: Change },
    ) {
      area(parent, "merge-left", options.left, true);
      const right = area(parent, "merge-right", options.right, false);
      right.addEventListener("input", () => options.onChange?.(right.value));
      return handle(right);
    },
    createDiffView(parent: HTMLElement, options: { before: string; after: string }) {
      area(parent, "diff-before", options.before, true);
      return handle(area(parent, "diff-after", options.after, true));
    },
  };
});

vi.mock("@/plan/editor/load", () => ({
  loadCodeMirror: () => Promise.resolve(fakeCodeMirror),
}));

const ME = { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" };
const doc: DocView = docViewSchema.parse(docFixture);
const SAVED = { result: "saved", hash: "sha256:new", validation: null };

function renderEdit() {
  const router = createMemoryRouter([{ path: "/files/*", element: <EditMode doc={doc} /> }], {
    initialEntries: [`/files/${doc.path}?edit=1`],
  });
  render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
  return router;
}

/** A fetch whose single reply the test releases; `json` records when the reply was read. */
function pendingFetch() {
  let release: () => void = () => {};
  const json = vi.fn(() => Promise.resolve(SAVED));
  const fetchMock = vi.fn(
    () =>
      new Promise((resolve) => {
        release = () => resolve({ ok: true, status: 200, json });
      }),
  );
  vi.stubGlobal("fetch", fetchMock);
  return { fetchMock, json, release: () => release() };
}

beforeEach(() => {
  localStorage.clear();
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
  store.set(docChangedAtom, {});
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("Save and close contract", () => {
  it("stays in the editor when text was typed during the save", async () => {
    const reply = pendingFetch();
    const router = renderEdit();
    const editor = (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;
    fireEvent.input(editor, { target: { value: "first edit" } });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save and close" }));
    await waitFor(() => expect(reply.fetchMock).toHaveBeenCalledTimes(1));

    fireEvent.input(screen.getByTestId("source-editor"), { target: { value: "second edit" } });
    await act(async () => reply.release());
    await waitFor(() => expect(reply.json).toHaveBeenCalled());
    // Let the consumed reply's state updates and any navigation they trigger settle.
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));

    expect(router.state.location.search).toBe("?edit=1");
    const current = screen.getByTestId("source-editor") as HTMLTextAreaElement;
    expect(current.value).toBe("second edit");
  });
});
