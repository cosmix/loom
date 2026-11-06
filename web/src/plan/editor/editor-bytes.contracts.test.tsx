import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { docChangedAtom, validationAtom } from "@/plan/files/atoms";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { EditMode } from "./edit-mode";
import { requestEditAtom } from "./request-edit";

// The byte rule end to end: the real CodeMirror chunk through the real loader, no `./load` mock.

beforeAll(() => {
  // jsdom has no text layout, and CodeMirror measures through Range rectangles.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 0, 0);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

const ME = { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" };

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
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  localStorage.clear();
});

/** Captures the body of every save request; each answers as a clean save. */
function stubSave(): string[] {
  const bodies: string[] = [];
  const saved = { result: "saved", hash: "sha256:saved", validation: null };
  vi.stubGlobal(
    "fetch",
    vi.fn((url: string, init?: RequestInit) => {
      if (String(url).includes("/api/save")) bodies.push(String(init?.body));
      return Promise.resolve({ ok: true, status: 200, json: () => Promise.resolve(saved) });
    }),
  );
  return bodies;
}

/** Opens `text` in the editor, toggles Source and back, presses Ctrl-S, returns the save bodies. */
async function saveUntouched(text: string): Promise<string[]> {
  const bodies = stubSave();
  const doc: DocView = { ...docViewSchema.parse(docFixture), text, size: text.length };
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
  const content = await vi.waitFor(() => {
    const found = document.querySelector<HTMLElement>(".cm-content");
    if (!found) throw new Error("editor not loaded");
    return found;
  });
  fireEvent.click(screen.getByRole("button", { name: "Source" }));
  fireEvent.click(screen.getByRole("button", { name: "Live preview" }));
  fireEvent.keyDown(content, { key: "s", ctrlKey: true });
  await vi.waitFor(() => expect(bodies).toHaveLength(1));
  return bodies;
}

it("saving an unedited document writes identical bytes", async () => {
  for (const text of ["# title\r\n\r\nbody  \r\n\r\n- [ ] item\r\n", "a\r\nb\nc\r"]) {
    const bodies = await saveUntouched(text);
    expect(bodies).toEqual([text]);
    cleanup();
    vi.unstubAllGlobals();
  }
});
