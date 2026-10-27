import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, useAtomValue } from "jotai";
import { Provider } from "jotai/react";
import { createMemoryRouter, MemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { initClient } from "@/plan/api/client";
import type { ConceptPage, Sentence } from "@/plan/api/schema-concept";
import type { AttachmentDraft } from "@/plan/attachments/types";

import { Announcer } from "./announce";
import { conceptPageAtom } from "./page-state";
import { refreshDocView } from "./passages";
import { PlanGlyph } from "./plan-glyph";
import { ReadingColumn } from "./reading-column";
import {
  cite,
  docLink,
  docOf,
  mark,
  pageOf,
  rect,
  S1,
  S2,
  sent,
  src,
  TEXT,
} from "./reading-column-fixtures";
import { notesAtom, openRunsAtom } from "./view-state";

let docs: Record<string, unknown> = {};

function stubFetch(input: RequestInfo | URL): Promise<Response> {
  const url = new URL(
    typeof input === "string" ? input : input instanceof URL ? input.href : input.url,
    "http://x",
  );
  const doc = url.pathname === "/api/doc" ? docs[url.searchParams.get("path") ?? ""] : undefined;
  const body = doc ?? { error: "not_found", message: "not found" };
  return Promise.resolve(new Response(JSON.stringify(body), { status: doc ? 200 : 404 }));
}

function Harness({
  pageKey,
  onAttach,
}: {
  pageKey: string;
  onAttach?: (d: AttachmentDraft) => void;
}) {
  const page = useAtomValue(conceptPageAtom(pageKey)).page;
  return page ? <ReadingColumn pageKey={pageKey} page={page} onAttach={onAttach} /> : null;
}

function mount(pageKey: string, page: ConceptPage, onAttach?: (d: AttachmentDraft) => void) {
  const store = createStore();
  initClient(store);
  store.set(conceptPageAtom(pageKey), { page, previous: null, error: null });
  const router = createMemoryRouter([
    { path: "*", element: <Harness pageKey={pageKey} onAttach={onAttach} /> },
  ]);
  render(
    <Provider store={store}>
      <RouterProvider router={router} />
    </Provider>,
  );
  return { store, router, column: screen.getByRole("document", { name: "Reading column" }) };
}

function replace(store: ReturnType<typeof createStore>, key: string, next: ConceptPage) {
  act(() => {
    const page = store.get(conceptPageAtom(key)).page;
    store.set(conceptPageAtom(key), { page: next, previous: page, error: null });
  });
}

/** An overview whose sentence "o" cites sentence "x" of section_01, loaded in the store. */
function nested(extraOverview: Partial<Sentence> = {}) {
  const o = {
    ...sent("o", "Overview opens.", [cite("section_01", "x")], [mark("section_01", "Planning", 1)]),
    ...extraOverview,
  };
  const overview = pageOf("product", [o]);
  const detail = pageOf("product/section_01", [sent("x", "Detail opens.", [src(S1, "intro")])]);
  const view = mount("product", overview);
  act(() =>
    view.store.set(conceptPageAtom("product/section_01"), {
      page: detail,
      previous: null,
      error: null,
    }),
  );
  return { ...view, overview, detail };
}

const press = (key: string) => fireEvent.keyDown(document.activeElement ?? document.body, { key });
const active = () => document.activeElement as HTMLElement;
const button = (column: HTMLElement, name: RegExp | string) =>
  within(column).getByRole("button", { name });
const group = (column: HTMLElement) => within(column).findByRole("group", { name: /^Detail of/ });
const two = [src(S2, "intro"), src(S2, "detail")];

beforeEach(() => {
  docs = { "product/section_02.md": docOf(S2), "product/section_01.md": docOf(S1) };
  window.sessionStorage.clear();
  vi.stubGlobal("fetch", vi.fn(stubFetch));
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("level-0 source citation opens passages", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Plans are reviewed.", [src(S2, "intro")])]),
  );
  fireEvent.click(button(column, /^Plans are reviewed/));
  const head = await within(column).findByRole("link", { name: /section_02\.md.*Intro/ });
  expect(head.closest("[data-passage]")?.getAttribute("data-passage")).toBe(`${S2}#intro`);
  expect(await within(column).findByText("Alpha text")).toBeTruthy();
});

it("level-0 sentence citing two documents shows two marks in order", () => {
  const marks = [mark("section_01", "Planning", 1), mark("section_02", "Review", 2)];
  const { column } = mount(
    "product",
    pageOf("product", [
      sent("a", "Both.", [cite("section_01", "x"), cite("section_02", "y")], marks),
    ]),
  );
  const labels = Array.from(column.querySelectorAll(".concept-mark")).map((el) =>
    el.getAttribute("aria-label"),
  );
  expect(labels).toEqual([`Planning: ${S1}`, `Review: ${S2}`]);
});

it("rebuilding mark dashed with reason text", () => {
  const marks = [mark("section_02", "Review", 2, { reason: "rebuilding", detail: null })];
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Both.", [src(S2, "intro")], marks)]),
  );
  const square = column.querySelector(".concept-mark")!;
  expect(square.getAttribute("data-dashed")).toBe("true");
  expect(square.getAttribute("aria-label")).toBe(`Review: ${S2}, out of date: rebuilding`);
});

it("mark tooltip names the reason", async () => {
  const marks = [mark("section_02", "Review", 2, { reason: "awaiting_approval", detail: null })];
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Both.", [src(S2, "intro")], marks)]),
  );
  fireEvent.focus(column.querySelector(".concept-mark")!);
  expect((await screen.findAllByText(/Out of date: waiting for approval/)).length).toBeGreaterThan(
    0,
  );
});

it("click ending a selection does not open", () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Selected text.", [src(S2, "intro")])]),
  );
  const selection = { isCollapsed: false, toString: () => "Selected" } as unknown as Selection;
  vi.spyOn(window, "getSelection").mockReturnValue(selection);
  fireEvent.click(button(column, /^Selected text/));
  expect(within(column).queryByRole("group")).toBeNull();
  expect(button(column, /^Selected text/).getAttribute("aria-expanded")).toBe("false");
});

it("mark click navigates", () => {
  const marks = [mark("section_02", "Review", 2)];
  const { column, router } = mount(
    "product",
    pageOf("product", [sent("a", "Marked.", [src(S2, "intro")], marks)]),
  );
  fireEvent.click(column.querySelector(".concept-mark")!);
  expect(router.state.location.pathname).toBe("/product/section_02");
});

it("passage header link carries the line range", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Ranged.", [src(S2, "intro")])]),
  );
  fireEvent.click(button(column, /^Ranged/));
  const head = await within(column).findByRole("link", { name: /Intro/ });
  expect(head.getAttribute("href")).toBe("/files/product/section_02.md?lines=3-5#intro");
});

it("no source mark, not a button", () => {
  const { column } = mount("product", pageOf("product", [sent("a", "Unsourced claim.")]));
  expect(within(column).queryByRole("button", { name: /Unsourced/ })).toBeNull();
  expect(within(column).getByText("no source")).toBeTruthy();
});

it("paragraph resumes after two stacked passages", async () => {
  const sentences = [
    sent("b", "Before it."),
    sent("a", "Opened one.", two),
    sent("c", "After it."),
  ];
  const { column } = mount("product", pageOf("product", sentences));
  fireEvent.click(button(column, /^Opened one/));
  expect(await within(column).findAllByRole("link", { name: /section_02\.md/ })).toHaveLength(2);
  const before = within(column).getByText("Before it.");
  const open = await group(column);
  const after = within(column).getByText("After it.");
  expect(before.compareDocumentPosition(open) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(open.compareDocumentPosition(after) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(after.closest(".concept-paragraph")).toBe(before.closest(".concept-paragraph"));
});

it("inset has no border-left style other than the hairline", async () => {
  const { column } = mount("product", pageOf("product", [sent("a", "Opened one.", two)]));
  fireEvent.click(button(column, /^Opened one/));
  const inset = (await group(column)).querySelector<HTMLElement>(".concept-passage")!;
  expect(inset.style.borderLeft).toBe("");
  expect(`${inset.className} ${inset.parentElement!.className}`).not.toMatch(
    /border-(l|s)\b|spine/,
  );
});

it("opened run carries the wash class", async () => {
  const { column } = nested();
  fireEvent.click(button(column, /^Overview opens/));
  expect((await group(column)).className).toContain("concept-run");
});

it("opening scrolls only when the opened point leaves the viewport", async () => {
  const sentences = [
    sent("a", "First open.", [src(S2, "intro")]),
    sent("b", "Second open.", [src(S2, "detail")]),
  ];
  const { column } = mount("product", pageOf("product", sentences));
  const scrollBy = vi.spyOn(window, "scrollBy").mockImplementation(() => {});
  const rects = vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue(rect(100));
  fireEvent.click(button(column, /^First open/));
  await group(column);
  expect(scrollBy).not.toHaveBeenCalled();
  rects.mockReturnValue(rect(window.innerHeight + 200));
  fireEvent.click(button(column, /^Second open/));
  await waitFor(() => expect(scrollBy).toHaveBeenCalled());
});

it("handle folding", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Foldable one.", [src(S2, "intro")])]),
  );
  fireEvent.click(button(column, /^Foldable one/));
  await group(column);
  fireEvent.click(button(column, "Fold"));
  expect(within(column).queryByRole("group")).toBeNull();
  await waitFor(() => expect(active().textContent).toMatch(/^Foldable one/));
});

it("nested runs have nested handles", async () => {
  const { column } = nested();
  fireEvent.click(button(column, /^Overview opens/));
  fireEvent.click(await within(column).findByRole("button", { name: /^Detail opens/ }));
  await within(column).findByRole("group", { name: /^Detail of Detail/ });
  const depths = within(column)
    .getAllByRole("button", { name: "Fold" })
    .map((b) => b.getAttribute("data-depth"));
  expect(depths.sort()).toEqual(["0", "1"]);
});

it("ArrowLeft folds like Escape", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Leftward one.", [src(S2, "intro")])]),
  );
  fireEvent.click(button(column, /^Leftward one/));
  await group(column);
  press("ArrowLeft");
  await waitFor(() => expect(within(column).queryByRole("group")).toBeNull());
  expect(active().textContent).toMatch(/^Leftward one/);
});

it("j and k move like the arrows", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "First line."), sent("b", "Second line.")]),
  );
  column.focus();
  await waitFor(() => expect(active().textContent).toBe("First line."));
  press("j");
  await waitFor(() => expect(active().textContent).toBe("Second line."));
  press("k");
  await waitFor(() => expect(active().textContent).toBe("First line."));
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toBe("Second line."));
});

it("ArrowRight opens", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Rightward one.", [src(S2, "intro")])]),
  );
  column.focus();
  await waitFor(() => expect(active().textContent).toMatch(/^Rightward/));
  press("ArrowRight");
  expect(await group(column)).toBeTruthy();
});

it("f and d navigation targets", async () => {
  const marks = [mark("section_02", "Review", 2)];
  const { column, router } = mount(
    "product",
    pageOf("product", [sent("a", "Targets here.", [src(S2, "intro")], marks)]),
  );
  column.focus();
  await waitFor(() => expect(active().textContent).toMatch(/^Targets/));
  press("d");
  expect(router.state.location.pathname).toBe("/product/section_02");
  press("f");
  expect(router.state.location.pathname).toBe("/files/product/section_02.md");
  expect(router.state.location.hash).toBe("#intro");
});

it("keys ignored while an input has focus", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Typed over.", [src(S2, "intro")])]),
  );
  const input = document.createElement("input");
  column.appendChild(input);
  input.focus();
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.keyDown(input, { key: "j" });
  expect(within(column).queryByRole("group")).toBeNull();
  expect(document.activeElement).toBe(input);
});

const pair = () => [sent("a", "Stays put."), sent("b", "Goes away.")];
const flashed = (column: HTMLElement, text: string) =>
  within(column).getByText(text).className.includes("concept-flash");

it("no flash on unchanged sentences", () => {
  const { column, store } = mount("product", pageOf("product", pair()));
  replace(store, "product", pageOf("product", [pair()[0]!, sent("c", "Brand new.")]));
  expect(flashed(column, "Stays put.")).toBe(false);
});

it("new sentence flashes", () => {
  vi.useFakeTimers();
  const { column, store } = mount("product", pageOf("product", pair()));
  replace(store, "product", pageOf("product", [pair()[0]!, sent("c", "Brand new.")]));
  expect(flashed(column, "Brand new.")).toBe(true);
  act(() => void vi.advanceTimersByTime(1500));
  expect(flashed(column, "Brand new.")).toBe(false);
});

it("passage flashes on hash change", async () => {
  const { column, store } = mount(
    "product",
    pageOf("product", [sent("a", "Flashy one.", [src(S2, "intro")])]),
  );
  fireEvent.click(button(column, /^Flashy one/));
  await within(column).findByText("Alpha text");
  docs["product/section_02.md"] = docOf(S2, TEXT, "h2");
  act(() => refreshDocView(store, S2));
  await waitFor(() =>
    expect(column.querySelector(".concept-passage.concept-flash")).not.toBeNull(),
  );
});

it("reduced motion", () => {
  vi.useFakeTimers();
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: query.includes("reduce"),
    media: query,
    addEventListener() {},
    removeEventListener() {},
  }));
  const { column, store } = mount("product", pageOf("product", pair()));
  replace(store, "product", pageOf("product", [pair()[0]!, sent("c", "Brand new.")]));
  act(() => void vi.advanceTimersByTime(1500));
  expect(flashed(column, "Brand new.")).toBe(true);
  act(() => void vi.advanceTimersByTime(1600));
  expect(flashed(column, "Brand new.")).toBe(false);
});

it("anchors on the focused sentence across a replacement", async () => {
  let top = 100;
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(function (this: Element) {
    return rect(this.getAttribute("data-sentence-id") === "a" ? top : 0);
  });
  const scrollBy = vi.spyOn(window, "scrollBy").mockImplementation(() => {});
  const { column, store } = mount("product", pageOf("product", pair()));
  column.focus();
  await waitFor(() => expect(active().textContent).toBe("Stays put."));
  top = 160;
  replace(store, "product", pageOf("product", pair(), { title: "Changed" }));
  expect(scrollBy).toHaveBeenCalledWith(0, 60);
});

it("note links to file mode", () => {
  const { column, store } = mount("product", pageOf("product", pair()));
  act(() => {
    store.set(notesAtom("product"), [
      { paragraphOf: "a", text: "Section Intro was removed from section_02.md", path: S2 },
    ]);
  });
  const link = within(column).getByRole("link", { name: /in file mode/ });
  expect(link.getAttribute("href")).toBe("/files/product/section_02.md");
});

it("announce throttled to ten seconds", () => {
  vi.useFakeTimers();
  vi.setSystemTime(0);
  const { rerender } = render(<Announcer title="Product" arrival={{ count: 2, seq: 1 }} />);
  const text = () => screen.getByRole("status").textContent;
  expect(text()).toBe("Product: 2 sentences updated");
  rerender(<Announcer title="Product" arrival={{ count: 3, seq: 2 }} />);
  expect(text()).toBe("Product: 2 sentences updated");
  act(() => void vi.advanceTimersByTime(10_000));
  expect(text()).toBe("Product: 3 sentences updated");
});

it("plan glyph popover links", async () => {
  const plans = [{ slug: "feat-x", title: "Feature X", state: "running" }];
  render(
    <MemoryRouter>
      <PlanGlyph plans={plans} sections={[]} />
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: /Plans implementing/ }));
  const link = await screen.findByRole("link", { name: "Feature X" });
  expect(link.getAttribute("href")).toBe("/engineering/feat-x");
});

it("a relative link in a passage opens file mode", async () => {
  docs["product/section_02.md"] = docOf(S2, "# Doc\n\n## Intro\nSee [Port](section_03.md#port).\n");
  const chips = [
    {
      doc: docLink("section_03", "Release"),
      colour: 3,
      label: "03",
      out_of_date: null,
      no_plan: false,
    },
  ];
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Linked one.", [src(S2, "intro")])], { key_chips: chips }),
  );
  fireEvent.click(button(column, /^Linked one/));
  await waitFor(() =>
    expect(column.querySelector('a[href="/files/product/section_03.md#port"]')).not.toBeNull(),
  );
});

it("two passages of one document produce no duplicate ids", async () => {
  const { column } = mount("product", pageOf("product", [sent("a", "Twin cut.", two)]));
  fireEvent.click(button(column, /^Twin cut/));
  await waitFor(() => expect(column.querySelectorAll(".concept-passage h2")).toHaveLength(2));
  const ids = Array.from(column.querySelectorAll("[id]")).map((el) => el.id);
  expect(new Set(ids).size).toBe(ids.length);
});

it("passage heading carries no id", async () => {
  const { column } = mount(
    "product",
    pageOf("product", [sent("a", "Headed cut.", [src(S2, "intro")])]),
  );
  fireEvent.click(button(column, /^Headed cut/));
  const heading = await within(column).findByRole("heading", { name: "Intro" });
  expect(heading.hasAttribute("id")).toBe(false);
});

it("Fold all moves focus to the outer sentence", async () => {
  const { column, store } = nested();
  fireEvent.click(button(column, /^Overview opens/));
  await within(column).findByRole("button", { name: /^Detail opens/ });
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toMatch(/^Detail opens/));
  act(() => store.set(openRunsAtom("product"), []));
  await waitFor(() => expect(active().textContent).toMatch(/^Overview opens/));
  expect(column.contains(active())).toBe(true);
});

it("a rebuild that folds the focused run moves focus to the restored sentence", async () => {
  const rests = (anchor: string) => ({ rests_on: [{ path: S1, anchor }] });
  const { column, store } = nested(rests("intro"));
  fireEvent.click(button(column, /^Overview opens/));
  await within(column).findByRole("button", { name: /^Detail opens/ });
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toMatch(/^Detail opens/));
  const rewritten = {
    ...sent("o", "Overview opens.", [cite("section_01", "x")], [mark("section_01", "Planning", 1)]),
    ...rests("other"),
  };
  replace(store, "product", pageOf("product", [rewritten]));
  await waitFor(() => expect(active().textContent).toMatch(/^Overview opens/));
  expect(within(column).getByRole("note")).toBeTruthy();
});
