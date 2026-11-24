import { cleanup, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { docChangedAtom, validationAtom } from "@/plan/files/atoms";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { EditMode } from "./edit-mode";
import { requestEditAtom } from "./request-edit";

// Shared setup for the EditMode test files. Each test file owns its `vi.mock` of the editor
// loader (vitest hoists mocks per file) and returns `fake` from it; this kit holds the rest.

// Each CodeMirror view is a textarea, so the tests read editor text from the DOM.
export const fake = (() => {
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
    let mode: "preview" | "source" = "preview";
    return {
      destroy: () => element.remove(),
      getText: () => element.value,
      setText: (text: string) => {
        element.value = text;
      },
      searchOpen: () => false,
      setGutterMarks: () => {},
      scrollToLine: (line: number) => scrolled.push(line),
      rangeLines: () => null,
      rangeText: () => null,
      mode: () => mode,
      setMode: (next: "preview" | "source") => {
        mode = next;
      },
      format: () => {},
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
})();

export const ME = { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" };
export const OTHER = {
  public_id: "bbbbbbbbbbbbbbbb",
  name: "Bo",
  away_since: null,
  frees_at: null,
};
export const doc: DocView = docViewSchema.parse(docFixture);

export function renderEdit() {
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

export const source = async () =>
  (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;

/** Registers the per-test store seeding and teardown the EditMode tests share. */
export function setupEditMode() {
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
}
