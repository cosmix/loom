import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import invalidFixture from "@/plan/api/fixtures/files-save-invalid.json";
import { docViewSchema, validationViewSchema, type DocView } from "@/plan/api/schema-files";
import { docRel } from "@/plan/files/address";
import { docChangedAtom, validationAtom } from "@/plan/files/atoms";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { findDrafts, loadDraft } from "./drafts";
import { EditMode } from "./edit-mode";
import { requestEditAtom } from "./request-edit";

// Each CodeMirror view is a textarea, so the tests read editor text from the DOM.
const fake = vi.hoisted(() => {
  const scrolled: number[] = [];
  function area(parent: HTMLElement, testId: string, value: string, readOnly: boolean) {
    const element = document.createElement("textarea");
    element.dataset.testid = testId;
    element.value = value;
    element.readOnly = readOnly;
    parent.append(element);
    return element;
  }
  function handle(element: HTMLTextAreaElement) {
    return {
      destroy: () => element.remove(),
      getText: () => element.value,
      setText: (text: string) => {
        element.value = text;
      },
      searchOpen: () => false,
      setGutterMarks: () => {},
      scrollToLine: (line: number) => scrolled.push(line),
    };
  }
  function onCtrlS(element: HTMLElement, save?: () => void) {
    element.addEventListener("keydown", (event) => {
      if (event.key === "s" && event.ctrlKey) save?.();
    });
  }
  return {
    scrolled,
    createSourceEditor(
      parent: HTMLElement,
      options: {
        doc: string;
        readOnly?: boolean;
        onChange: (t: string) => void;
        onSave?: () => void;
      },
    ) {
      const element = area(parent, "source-editor", options.doc, options.readOnly ?? false);
      element.addEventListener("input", () => options.onChange(element.value));
      onCtrlS(element, options.onSave);
      return handle(element);
    },
    createMergeView(
      parent: HTMLElement,
      options: {
        left: string;
        right: string;
        onChange?: (t: string) => void;
        onSave?: () => void;
      },
    ) {
      area(parent, "merge-left", options.left, true);
      const right = area(parent, "merge-right", options.right, false);
      right.addEventListener("input", () => options.onChange?.(right.value));
      onCtrlS(right, options.onSave);
      return handle(right);
    },
  };
});

vi.mock("@/plan/editor/load", () => ({ loadCodeMirror: () => Promise.resolve(fake) }));

const ME = { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" };
const OTHER = { public_id: "bbbbbbbbbbbbbbbb", name: "Bo", away_since: null, frees_at: null };
const doc: DocView = docViewSchema.parse(docFixture);

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

const source = async () => (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;

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
  store.set(requestEditAtom, null);
  store.set(validationAtom, {});
  fake.scrolled.length = 0;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("EditMode", () => {
  it("says unsaved text is not kept when storage is full", async () => {
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw Object.assign(new Error("full"), { name: "QuotaExceededError" });
    });
    renderEdit();
    fireEvent.input(await source(), { target: { value: "changed" } });
    expect(await screen.findByText(/not being kept: browser storage is full/)).toBeTruthy();
  });

  it("turns read-only when authoring is lost and editable again when it returns", async () => {
    renderEdit();
    const editor = await source();
    fireEvent.input(editor, { target: { value: "kept words" } });
    act(() => store.set(authoringAtom, { holder: OTHER }));

    await waitFor(() =>
      expect((screen.getByTestId("source-editor") as HTMLTextAreaElement).readOnly).toBe(true),
    );
    expect((screen.getByTestId("source-editor") as HTMLTextAreaElement).value).toBe("kept words");
    expect(screen.getByText("Bo took authoring. Your text is kept as a draft.")).toBeTruthy();
    const save = screen.getByRole("button", { name: "Save" });
    expect(document.getElementById(save.getAttribute("aria-describedby") ?? "")?.textContent).toBe(
      "Bo holds authoring",
    );

    act(() => store.set(authoringAtom, { holder: { ...ME, away_since: null, frees_at: null } }));
    await waitFor(() =>
      expect((screen.getByTestId("source-editor") as HTMLTextAreaElement).readOnly).toBe(false),
    );
    expect((screen.getByTestId("source-editor") as HTMLTextAreaElement).value).toBe("kept words");
  });

  it("keeps the text in a panel when the open file is deleted", async () => {
    renderEdit();
    fireEvent.input(await source(), { target: { value: "precious" } });
    act(() => store.set(docChangedAtom, { [doc.path]: null }));
    expect(await screen.findByText(/This file was deleted; your text is kept/)).toBeTruthy();
    expect((screen.getByLabelText("Your kept text") as HTMLTextAreaElement).value).toBe("precious");
    expect(screen.getByRole("button", { name: "Copy" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Discard" })).toBeTruthy();
  });

  it("offers the comparison as a line that does not take focus", async () => {
    renderEdit();
    const editor = await source();
    editor.focus();
    act(() => store.set(docChangedAtom, { [doc.path]: "sha256:elsewhere" }));
    const line = await screen.findByText(/Changed on disk/);
    expect(line.getAttribute("role")).toBe("note");
    expect(document.activeElement).toBe(editor);
    expect(screen.getByRole("button", { name: "Compare" })).toBeTruthy();
  });

  it("opens the merge with the requested text on the right when its hash is stale", async () => {
    store.set(requestEditAtom, { path: doc.path, text: "restored text", openedHash: "sha256:old" });
    renderEdit();
    expect(((await screen.findByTestId("merge-left")) as HTMLTextAreaElement).value).toBe(doc.text);
    expect(((await screen.findByTestId("merge-right")) as HTMLTextAreaElement).value).toBe(
      "restored text",
    );
    expect(screen.queryByTestId("source-editor")).toBeNull();
    await waitFor(() => expect(store.get(requestEditAtom)).toBeNull());
  });

  it("asks to save or discard on Done and on Escape with unsaved text", async () => {
    const router = renderEdit();
    const editor = await source();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    fireEvent.input(editor, { target: { value: "unsaved" } });
    fireEvent.keyDown(editor, { key: "Escape" });
    expect(await screen.findByRole("alertdialog")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Save and close" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(await screen.findByRole("alertdialog")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
  });

  it("leaves at once on Done with nothing unsaved, and ignores Escape inside the search panel", async () => {
    const router = renderEdit();
    const editor = await source();
    const panel = document.createElement("div");
    panel.className = "cm-search";
    editor.parentElement?.append(panel);
    fireEvent.keyDown(editor, { key: "Escape" });
    expect(router.state.location.search).toBe("?edit=1");
    panel.remove();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
  });

  it("blocks navigation away with unsaved text", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    const router = renderEdit();
    fireEvent.input(await source(), { target: { value: "unsaved" } });
    await act(() => router.navigate("/files/other.md"));
    expect(confirm).toHaveBeenCalled();
    expect(router.state.location.pathname).toContain(doc.path);
  });

  it("saves on Ctrl-S while authoring and never posts once authoring is lost", async () => {
    const post = vi.fn(() => new Promise<Response>(() => {}));
    vi.stubGlobal("fetch", post);
    renderEdit();
    const editor = await source();
    fireEvent.input(editor, { target: { value: "changed" } });
    act(() => store.set(authoringAtom, { holder: OTHER }));
    await waitFor(() =>
      expect(screen.getByTestId("source-editor")).toHaveProperty("readOnly", true),
    );
    fireEvent.keyDown(screen.getByTestId("source-editor"), { key: "s", ctrlKey: true });
    expect(post).not.toHaveBeenCalled();

    act(() => store.set(authoringAtom, { holder: { ...ME, away_since: null, frees_at: null } }));
    await waitFor(() =>
      expect(screen.getByTestId("source-editor")).toHaveProperty("readOnly", false),
    );
    fireEvent.keyDown(screen.getByTestId("source-editor"), { key: "s", ctrlKey: true });
    await waitFor(() => expect(post).toHaveBeenCalledTimes(1));
  });

  it("scrolls the editor to the line of a validation finding", async () => {
    store.set(validationAtom, {
      [doc.path]: validationViewSchema.parse({
        ...invalidFixture.validation,
        errors: [{ message: "bad stage", stage_id: null, line: 40, column: null }],
      }),
    });
    renderEdit();
    await source();
    fireEvent.click(screen.getByRole("button", { name: "line 40" }));
    expect(fake.scrolled).toEqual([40]);
  });

  it("keeps the same editor when the document is saved and its hash moves on", async () => {
    renderEdit();
    const editor = await source();
    fireEvent.input(editor, { target: { value: "typed" } });
    act(() => store.set(docChangedAtom, { [doc.path]: "sha256:other" }));
    expect(screen.getByTestId("source-editor")).toBe(editor);
    expect(editor.value).toBe("typed");
  });
});

describe("EditMode drafts and requests", () => {
  it("writes one draft after typing pauses instead of one per keystroke", async () => {
    renderEdit();
    const editor = await source();
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    for (const value of ["a", "ab", "abc"]) fireEvent.input(editor, { target: { value } });
    expect(setItem).not.toHaveBeenCalled();
    await waitFor(() => expect(loadDraft(doc.path, doc.hash)).toBe("abc"), { timeout: 2000 });
    expect(setItem).toHaveBeenCalledTimes(1);
  });

  it("keeps the pending draft when the editor goes away", async () => {
    renderEdit();
    fireEvent.input(await source(), { target: { value: "kept" } });
    cleanup();
    expect(loadDraft(doc.path, doc.hash)).toBe("kept");
  });

  it("does not bring a discarded draft back", async () => {
    const router = renderEdit();
    fireEvent.input(await source(), { target: { value: "unsaved" } });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Discard" }));
    await waitFor(() => expect(router.state.location.search).toBe(""));
    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(findDrafts(doc.path)).toEqual([]);
  });

  it("opens the merge for a request that names the document doc-relative", async () => {
    store.set(requestEditAtom, {
      path: docRel(doc.path),
      text: "restored",
      openedHash: "sha256:old",
    });
    renderEdit();
    expect(((await screen.findByTestId("merge-right")) as HTMLTextAreaElement).value).toBe(
      "restored",
    );
    await waitFor(() => expect(store.get(requestEditAtom)).toBeNull());
  });

  it("opens the merge read-only while another browser holds authoring", async () => {
    store.set(authoringAtom, { holder: OTHER });
    store.set(requestEditAtom, { path: doc.path, text: "restored", openedHash: "sha256:old" });
    const created = vi.spyOn(fake, "createMergeView");
    renderEdit();
    await screen.findByTestId("merge-right");
    expect(created).toHaveBeenCalledWith(
      expect.anything(),
      expect.objectContaining({ readOnly: true }),
    );
  });

  it("moves focus into the unsaved-changes choice and back when it closes", async () => {
    renderEdit();
    fireEvent.input(await source(), { target: { value: "unsaved" } });
    const done = screen.getByRole("button", { name: "Done" });
    done.focus();
    fireEvent.click(done);
    const save = await screen.findByRole("button", { name: "Save and close" });
    expect(document.activeElement).toBe(save);
    fireEvent.click(screen.getByRole("button", { name: "Keep editing" }));
    expect(document.activeElement).toBe(done);
  });
});
