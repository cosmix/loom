import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import planDocFixture from "@/plan/api/fixtures/files-doc-plan.json";
import conflictFixture from "@/plan/api/fixtures/files-save-conflict.json";
import invalidFixture from "@/plan/api/fixtures/files-save-invalid.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { saveDraft } from "@/plan/editor/drafts";
import { EditMode } from "@/plan/editor/edit-mode";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

// A stand-in for the lazy CodeMirror chunk: each view is plain textareas, so the
// contracts read the editor text and the two merge sides from the DOM.
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
    return {
      destroy: () => element.remove(),
      getText: () => element.value,
      setText: (text: string) => {
        element.value = text;
      },
      searchOpen: () => false,
    };
  }
  type Change = (text: string) => void;
  return {
    createSourceEditor(
      parent: HTMLElement,
      options: { doc: string; readOnly?: boolean; onChange: Change; onSave: () => void },
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
const sectionDoc: DocView = docViewSchema.parse(docFixture);
const planDoc: DocView = docViewSchema.parse(planDocFixture);

function renderEditMode(doc: DocView) {
  const router = createMemoryRouter([{ path: "/files/*", element: <EditMode doc={doc} /> }], {
    initialEntries: [`/files/${doc.path}?edit=1`],
  });
  return render(
    <Provider store={store}>
      <TooltipProvider>
        <RouterProvider router={router} />
      </TooltipProvider>
    </Provider>,
  );
}

function answer(status: number, body: unknown) {
  return { ok: status >= 200 && status < 300, status, json: () => Promise.resolve(body) };
}

function stubFetch(...replies: [number, unknown][]) {
  const queue = [...replies];
  const fetchMock = vi.fn(() => {
    const [status, body] = queue.shift() ?? [500, { error: "unexpected" }];
    return Promise.resolve(answer(status, body));
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

function savedQuery(fetchMock: ReturnType<typeof stubFetch>, call: number): URLSearchParams {
  const [url] = fetchMock.mock.calls[call] as unknown as [string];
  return new URL(url, "http://localhost").searchParams;
}

function clickSave() {
  const buttons = screen
    .getAllByRole("button", { name: /^save\b/i })
    .filter((button) => !/error/i.test(button.textContent ?? ""))
    .filter((button) => !(button as HTMLButtonElement).disabled);
  expect(buttons.length).toBeGreaterThan(0);
  fireEvent.click(buttons[0]);
}

async function editSource(text: string) {
  const source = (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;
  fireEvent.input(source, { target: { value: text } });
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
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("editor contracts", () => {
  it("editor opens the merge view when a save conflicts", async () => {
    const saved = { result: "saved", hash: "sha256:ghi", validation: null };
    const fetchMock = stubFetch([409, conflictFixture], [200, saved]);
    renderEditMode(sectionDoc);
    await editSource("# Product\n\nMy edit\n");
    clickSave();

    const left = (await screen.findByTestId("merge-left")) as HTMLTextAreaElement;
    const right = (await screen.findByTestId("merge-right")) as HTMLTextAreaElement;
    expect(left.value).toBe(conflictFixture.current_text);
    expect(right.value).toBe("# Product\n\nMy edit\n");
    expect(savedQuery(fetchMock, 0).get("hash")).toBe(sectionDoc.hash);

    clickSave();
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(savedQuery(fetchMock, 1).get("hash")).toBe(conflictFixture.current_hash);
  });

  it("editor sends with errors only after save with errors", async () => {
    const saved = { result: "saved", hash: "sha256:ghi", validation: invalidFixture.validation };
    const fetchMock = stubFetch([422, invalidFixture], [200, saved]);
    renderEditMode(planDoc);
    await editSource(`${planDoc.text}\nMore\n`);
    clickSave();

    const withErrors = await screen.findByRole("button", { name: /save with errors/i });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(savedQuery(fetchMock, 0).get("with_errors")).toBeNull();
    expect(screen.getByText(invalidFixture.validation.errors[0].message)).toBeTruthy();

    fireEvent.click(withErrors);
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(2));
    expect(savedQuery(fetchMock, 1).get("with_errors")).toBe("1");
  });

  it("editor restores a same hash draft and merges an older one", async () => {
    stubFetch();
    const draft = "# Product\n\nUnsaved draft words\n";
    expect(saveDraft(sectionDoc.path, sectionDoc.hash, draft)).toBe("saved");

    renderEditMode(sectionDoc);
    const source = (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;
    expect(source.value).toBe(draft);
    cleanup();

    const changed: DocView = { ...sectionDoc, hash: "sha256:changed", text: "# Product\n\nNew\n" };
    renderEditMode(changed);
    const left = (await screen.findByTestId("merge-left")) as HTMLTextAreaElement;
    const right = (await screen.findByTestId("merge-right")) as HTMLTextAreaElement;
    expect(left.value).toBe(changed.text);
    expect(right.value).toBe(draft);
    for (const element of screen.queryAllByTestId("source-editor")) {
      expect((element as HTMLTextAreaElement).value).not.toBe(draft);
    }
  });

  it("only the lazy loader imports codemirror", () => {
    const sources = import.meta.glob(
      [
        "/src/plan/**/*.{ts,tsx}",
        "/src/components/**/*.{ts,tsx}",
        "!**/*.test.ts",
        "!**/*.test.tsx",
      ],
      { query: "?raw", import: "default", eager: true },
    ) as Record<string, string>;
    const staticImport =
      /^\s*(?:import|export)\b[^;]*?\bfrom\s*["']@codemirror\/|^\s*import\s*["']@codemirror\//m;
    const loader = "/src/plan/editor/codemirror.ts";

    // The scan must see the one module that is allowed to import CodeMirror,
    // and the lazy loader must reach it through a dynamic import only.
    expect(Object.keys(sources)).toContain(loader);
    expect(staticImport.test(sources[loader] ?? "")).toBe(true);
    expect(sources["/src/plan/editor/load.ts"] ?? "").toMatch(
      /import\(\s*["']\.\/codemirror["']\s*\)/,
    );

    const offenders = Object.entries(sources)
      .filter(([file]) => file !== loader)
      .filter(([, text]) => staticImport.test(text))
      .map(([file]) => file);
    expect(offenders).toEqual([]);
  });
});
