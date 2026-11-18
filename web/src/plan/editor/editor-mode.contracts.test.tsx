import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import docFixture from "@/plan/api/fixtures/files-doc.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { pendingAttachmentsAtom } from "@/plan/attachments/atoms";
import { docChangedAtom, validationAtom } from "@/plan/files/atoms";
import { toAddress } from "@/plan/files/address";
import { authoringAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";
import { store } from "@/state/store";

import { EditMode } from "./edit-mode";
import { requestEditAtom } from "./request-edit";

// A stand-in for the lazy CodeMirror chunk that records what the React side asks of it.
const fake = vi.hoisted(() => {
  type Options = {
    doc: string;
    readOnly?: boolean;
    mode?: "preview" | "source";
    onChange: (text: string) => void;
    onSave: () => void;
    onFollowLink?: (href: string) => void;
  };
  const state = {
    calls: [] as Options[],
    modes: [] as string[],
    formats: [] as string[],
    surface: null as string | null,
    lines: null as { start: number; end: number } | null,
    rangeText: null as string | null,
  };
  function createSourceEditor(parent: HTMLElement, options: Options) {
    state.calls.push(options);
    const element = document.createElement("div");
    element.dataset.testid = "source-editor";
    element.textContent = state.surface ?? options.doc;
    parent.append(element);
    let text = options.doc;
    let mode = options.mode ?? "preview";
    return {
      destroy: () => element.remove(),
      getText: () => text,
      setText: (next: string) => {
        text = next;
      },
      searchOpen: () => false,
      scrollToLine: () => {},
      setGutterMarks: () => {},
      rangeLines: () => state.lines,
      rangeText: () => state.rangeText,
      mode: () => mode,
      setMode: (next: "preview" | "source") => {
        state.modes.push(next);
        mode = next;
      },
      format: (command: string) => state.formats.push(command),
    };
  }
  return { state, createSourceEditor };
});

vi.mock("@/plan/editor/load", () => ({ loadCodeMirror: () => Promise.resolve(fake) }));

const ME = { public_id: "aaaaaaaaaaaaaaaa", name: "Ada" };
const OTHER = { public_id: "bbbbbbbbbbbbbbbb", name: "Bo", away_since: null, frees_at: null };
const doc: DocView = docViewSchema.parse(docFixture);
const MODE_KEY = "loom-plan:example-1234:editor-mode";

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

const surface = () => screen.findByTestId("source-editor");
const button = (name: string) => screen.getByRole("button", { name }) as HTMLButtonElement;

beforeAll(() => {
  // jsdom has no layout; Add to chat places itself from the selection's rectangle.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 10, 10);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

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
  store.set(pendingAttachmentsAtom, []);
  Object.assign(fake.state, { surface: null, lines: null, rangeText: null });
  for (const list of [fake.state.calls, fake.state.modes, fake.state.formats]) list.length = 0;
});

afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
  vi.restoreAllMocks();
  localStorage.clear();
});

it("editor reopens in the last chosen mode", async () => {
  renderEdit();
  await surface();
  fireEvent.click(button("Source"));
  expect(localStorage.getItem(MODE_KEY)).toBe("source");
  cleanup();
  renderEdit();
  await surface();
  await waitFor(() => expect(fake.state.calls).toHaveLength(2));
  expect(fake.state.calls[1]!.mode).toBe("source");
});

it("toolbar toggle switches the live editor without rebuilding it", async () => {
  renderEdit();
  await surface();
  expect(fake.state.calls[0]!.mode).toBe("preview");
  const pressed = () =>
    [button("Live preview"), button("Source")].map((b) => b.getAttribute("aria-pressed"));
  expect(pressed()).toEqual(["true", "false"]);
  fireEvent.click(button("Source"));
  expect(pressed()).toEqual(["false", "true"]);
  fireEvent.click(button("Live preview"));
  expect(pressed()).toEqual(["true", "false"]);
  expect(fake.state.calls).toHaveLength(1);
  expect(fake.state.modes).toEqual(["source", "preview"]);
});

const FORMAT_BUTTONS: [string, string][] = [
  ["Bold", "bold"],
  ["Italic", "italic"],
  ["Code", "code"],
  ["Link", "link"],
  ["Heading", "heading"],
  ["Bulleted list", "bulletList"],
  ["Numbered list", "orderedList"],
  ["Task list", "taskList"],
  ["Quote", "quote"],
  ["Code block", "codeBlock"],
  ["Table", "table"],
];

it("toolbar formatting buttons reach the editor and disable when read-only", async () => {
  renderEdit();
  await surface();
  for (const [name] of FORMAT_BUTTONS) fireEvent.click(button(name));
  expect(fake.state.formats).toEqual(FORMAT_BUTTONS.map(([, command]) => command));

  act(() => store.set(authoringAtom, { holder: OTHER }));
  await waitFor(() => {
    for (const [name] of FORMAT_BUTTONS) expect(button(name).disabled).toBe(true);
  });
  expect(button("Source").disabled).toBe(false);
  expect(button("Live preview").disabled).toBe(false);
});

it("following a link routes through the resolver and the leave guard", async () => {
  const router = renderEdit();
  await surface();
  const follow = fake.state.calls[0]!.onFollowLink!;
  const at = () => router.state.location.pathname + router.state.location.hash;
  const open = vi.spyOn(window, "open").mockReturnValue(null);

  await act(async () => follow("section_02.md"));
  expect(at()).toBe(toAddress("doc/product/demo/section_02.md"));
  await act(async () => follow("#anchor"));
  const here = toAddress(doc.path, "anchor");
  expect(at()).toBe(here);
  await act(async () => follow("https://example.org/x"));
  expect(open).toHaveBeenCalledWith("https://example.org/x", "_blank", "noopener,noreferrer");
  expect(at()).toBe(here);
  // A non-link scheme resolves to plain text and a path above doc/ to outside: neither acts.
  for (const href of ["javascript:alert(1)", "../../../../etc/passwd"]) {
    await act(async () => follow(href));
  }
  expect(open).toHaveBeenCalledTimes(1);
  expect(at()).toBe(here);

  const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
  act(() => fake.state.calls[0]!.onChange("unsaved words"));
  await act(async () => follow("section_02.md"));
  expect(confirm).toHaveBeenCalled();
  expect(at()).toBe(here);
});

it("add to chat carries the source markers", async () => {
  Object.assign(fake.state, {
    surface: "some bold text",
    lines: { start: 1, end: 1 },
    rangeText: "some **bold** text",
  });
  renderEdit();
  const shown = await surface();
  expect(shown.textContent).toBe("some bold text");
  const range = document.createRange();
  range.selectNodeContents(shown);
  window.getSelection()!.removeAllRanges();
  window.getSelection()!.addRange(range);
  act(() => {
    document.dispatchEvent(new Event("selectionchange"));
  });
  fireEvent.click(await screen.findByRole("button", { name: "Add to chat" }));
  expect(store.get(pendingAttachmentsAtom)).toEqual([
    expect.objectContaining({
      kind: "source",
      text: "some **bold** text",
      lines: { start: 1, end: 1 },
    }),
  ]);
});
