import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { expect, it, vi } from "vitest";

import { Announcer } from "./announce";
import { refreshDocView } from "./passages";
import { PlanGlyph } from "./plan-glyph";
import { cite, docOf, pageOf, rect, S1, S2, sent, src, TEXT } from "./reading-column-fixtures";
import {
  active,
  button,
  docLink,
  docs,
  flashed,
  focusDetail,
  group,
  installColumnTestHooks,
  mark,
  mount,
  mountProduct,
  nested,
  openable,
  pair,
  press,
  replace,
  two,
} from "./reading-column-harness";
import { notesAtom, openRunsAtom } from "./view-state";

installColumnTestHooks();

it("level-0 source citation opens passages", async () => {
  const { column } = mountProduct([sent("a", "Plans are reviewed.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^Plans are reviewed/));
  const head = await within(column).findByRole("link", { name: /section_02\.md.*Intro/ });
  expect(head.closest("[data-passage]")?.getAttribute("data-passage")).toBe(`${S2}#intro`);
  expect(await within(column).findByText("Alpha text")).toBeTruthy();
});

it("level-0 sentence citing two documents shows two marks in order", () => {
  const marks = [mark("section_01", "Planning", 1), mark("section_02", "Review", 2)];
  const { column } = mountProduct([
    sent("a", "Both.", [cite("section_01", "x"), cite("section_02", "y")], marks),
  ]);
  const labels = Array.from(column.querySelectorAll(".concept-mark")).map((el) =>
    el.getAttribute("aria-label"),
  );
  expect(labels).toEqual([`Planning: ${S1}`, `Review: ${S2}`]);
});

it("rebuilding mark dashed with reason text", () => {
  const marks = [mark("section_02", "Review", 2, { reason: "rebuilding", detail: null })];
  const { column } = mountProduct([sent("a", "Both.", [src(S2, "intro")], marks)]);
  const square = column.querySelector(".concept-mark")!;
  expect(square.getAttribute("data-dashed")).toBe("true");
  expect(square.getAttribute("aria-label")).toBe(`Review: ${S2}, out of date: rebuilding`);
});

it("mark tooltip names the reason", async () => {
  const marks = [mark("section_02", "Review", 2, { reason: "awaiting_approval", detail: null })];
  const { column } = mountProduct([sent("a", "Both.", [src(S2, "intro")], marks)]);
  fireEvent.focus(column.querySelector(".concept-mark")!);
  expect((await screen.findAllByText(/Out of date: waiting for approval/)).length).toBeGreaterThan(
    0,
  );
});

it("click ending a selection does not open", () => {
  const { column } = mountProduct([sent("a", "Selected text.", [src(S2, "intro")])]);
  const selection = { isCollapsed: false, toString: () => "Selected" } as unknown as Selection;
  vi.spyOn(window, "getSelection").mockReturnValue(selection);
  fireEvent.click(button(column, /^Selected text/));
  expect(within(column).queryByRole("group")).toBeNull();
  expect(button(column, /^Selected text/).getAttribute("aria-expanded")).toBe("false");
});

it.each([
  ["demo/product/section_02", "/f/demo/product/section_02"],
  ["demo/engineering/feat-x", "/f/demo/engineering/feat-x"],
  ["plans/feat-x", "/plans/feat-x"],
])("mark click navigates from key %s to %s", (page, pathname) => {
  const marks = [mark("section_02", "Review", 2)];
  marks[0]!.doc.page = page;
  const { column, router } = mountProduct([sent("a", "Marked.", [src(S2, "intro")], marks)]);
  fireEvent.click(column.querySelector(".concept-mark")!);
  expect(router.state.location.pathname).toBe(pathname);
});

it("passage header link carries the line range", async () => {
  const { column } = mountProduct([sent("a", "Ranged.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^Ranged/));
  const head = await within(column).findByRole("link", { name: /Intro/ });
  expect(head.getAttribute("href")).toBe("/files/product/section_02.md?lines=3-5#intro");
});

it("no source mark, not a button", () => {
  const { column } = mountProduct([sent("a", "Unsourced claim.")]);
  expect(within(column).queryByRole("button", { name: /Unsourced/ })).toBeNull();
  expect(within(column).getByText("no source")).toBeTruthy();
});

it("paragraph resumes after two stacked passages", async () => {
  const sentences = [
    sent("b", "Before it."),
    sent("a", "Opened one.", two),
    sent("c", "After it."),
  ];
  const { column } = mountProduct(sentences);
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
  const { column } = mountProduct([sent("a", "Opened one.", two)]);
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
  const { column } = mountProduct(sentences);
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
  const { column } = mountProduct([sent("a", "Foldable one.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^Foldable one/));
  await group(column);
  fireEvent.click(within(column).getAllByRole("button", { name: "Fold" })[0]!);
  expect(within(column).queryByRole("group")).toBeNull();
  await waitFor(() => expect(active().textContent).toMatch(/^Foldable one/));
});

it("nested runs have their own caps", async () => {
  const { column } = nested();
  fireEvent.click(button(column, /^Overview opens/));
  fireEvent.click(await within(column).findByRole("button", { name: /^Detail opens/ }));
  await within(column).findByRole("group", { name: /^Detail of Detail/ });
  const caps = within(column).getAllByRole("button", { name: "Fold" });
  expect(caps).toHaveLength(4);
  expect(caps.map((cap) => cap.getAttribute("data-side"))).toEqual([
    "start",
    "start",
    "end",
    "end",
  ]);
  expect(caps[0]!.getAttribute("aria-describedby")).toBe(caps[3]!.getAttribute("aria-describedby"));
  expect(caps[1]!.getAttribute("aria-describedby")).toBe(caps[2]!.getAttribute("aria-describedby"));
  expect(caps[0]!.getAttribute("aria-describedby")).not.toBe(
    caps[1]!.getAttribute("aria-describedby"),
  );
});

it("ArrowLeft folds like Escape", async () => {
  const { column } = mountProduct([sent("a", "Leftward one.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^Leftward one/));
  await group(column);
  press("ArrowLeft");
  await waitFor(() => expect(within(column).queryByRole("group")).toBeNull());
  expect(active().textContent).toMatch(/^Leftward one/);
});

it("j and k move like the arrows", async () => {
  const { column } = mountProduct([
    openable("a", "First line."),
    // Another section, so the two lines are not one fragment.
    sent("b", "Second line.", [src(S2, "detail")]),
  ]);
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
  const { column } = mountProduct([sent("a", "Rightward one.", [src(S2, "intro")])]);
  column.focus();
  await waitFor(() => expect(active().textContent).toMatch(/^Rightward/));
  press("ArrowRight");
  expect(await group(column)).toBeTruthy();
});

it("f and d navigation targets", async () => {
  const marks = [mark("section_02", "Review", 2)];
  const { column, router } = mountProduct([sent("a", "Targets here.", [src(S2, "intro")], marks)]);
  column.focus();
  await waitFor(() => expect(active().textContent).toMatch(/^Targets/));
  press("d");
  expect(router.state.location.pathname).toBe("/f/demo/product/section_02");
  press("f");
  expect(router.state.location.pathname).toBe("/files/product/section_02.md");
  expect(router.state.location.hash).toBe("#intro");
});

it("keys ignored while an input has focus", async () => {
  const { column } = mountProduct([sent("a", "Typed over.", [src(S2, "intro")])]);
  const input = document.createElement("input");
  column.appendChild(input);
  input.focus();
  fireEvent.keyDown(input, { key: "Enter" });
  fireEvent.keyDown(input, { key: "j" });
  expect(within(column).queryByRole("group")).toBeNull();
  expect(document.activeElement).toBe(input);
});

it("no flash on unchanged sentences", () => {
  const { column, store } = mountProduct(pair());
  replace(
    store,
    "overview/demo/product",
    pageOf("overview/demo/product", [pair()[0]!, sent("c", "Brand new.")]),
  );
  expect(flashed(column, "Stays put.")).toBe(false);
});

it("new sentence flashes", () => {
  vi.useFakeTimers();
  const { column, store } = mountProduct(pair());
  replace(
    store,
    "overview/demo/product",
    pageOf("overview/demo/product", [pair()[0]!, sent("c", "Brand new.")]),
  );
  expect(flashed(column, "Brand new.")).toBe(true);
  act(() => void vi.advanceTimersByTime(1500));
  expect(flashed(column, "Brand new.")).toBe(false);
});

it("passage flashes on hash change", async () => {
  const { column, store } = mountProduct([sent("a", "Flashy one.", [src(S2, "intro")])]);
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
  const { column, store } = mountProduct(pair());
  replace(
    store,
    "overview/demo/product",
    pageOf("overview/demo/product", [pair()[0]!, sent("c", "Brand new.")]),
  );
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
  const { column, store } = mountProduct(pair());
  column.focus();
  await waitFor(() => expect(active().textContent).toBe("Stays put."));
  top = 160;
  replace(
    store,
    "overview/demo/product",
    pageOf("overview/demo/product", pair(), { title: "Changed" }),
  );
  expect(scrollBy).toHaveBeenCalledWith(0, 60);
});

it("note links to file mode", () => {
  const { column, store } = mountProduct(pair());
  act(() => {
    store.set(notesAtom("overview/demo/product"), [
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
  expect(link.getAttribute("href")).toBe("/plans/feat-x");
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
    "overview/demo/product",
    pageOf("overview/demo/product", [sent("a", "Linked one.", [src(S2, "intro")])], {
      key_chips: chips,
    }),
  );
  fireEvent.click(button(column, /^Linked one/));
  await waitFor(() =>
    expect(column.querySelector('a[href="/files/product/section_03.md#port"]')).not.toBeNull(),
  );
});

it("two passages of one document produce no duplicate ids", async () => {
  const { column } = mountProduct([sent("a", "Twin cut.", two)]);
  fireEvent.click(button(column, /^Twin cut/));
  await waitFor(() => expect(column.querySelectorAll(".concept-passage h2")).toHaveLength(2));
  const ids = Array.from(column.querySelectorAll("[id]")).map((el) => el.id);
  expect(new Set(ids).size).toBe(ids.length);
});

it("passage heading carries no id", async () => {
  const { column } = mountProduct([sent("a", "Headed cut.", [src(S2, "intro")])]);
  fireEvent.click(button(column, /^Headed cut/));
  const heading = await within(column).findByRole("heading", { name: "Intro" });
  expect(heading.hasAttribute("id")).toBe(false);
});

it("Fold all moves focus to the outer sentence", async () => {
  const { column, store } = nested();
  await focusDetail(column);
  act(() => store.set(openRunsAtom("overview/demo/product"), []));
  await waitFor(() => expect(active().textContent).toMatch(/^Overview opens/));
  expect(column.contains(active())).toBe(true);
});

it("a rebuild that folds the focused run moves focus to the restored sentence", async () => {
  const rests = (anchor: string) => ({ rests_on: [{ path: S1, anchor }] });
  const { column, store } = nested(rests("intro"));
  await focusDetail(column);
  const rewritten = {
    ...sent("o", "Overview opens.", [cite("section_01", "x")], [mark("section_01", "Planning", 1)]),
    ...rests("other"),
  };
  replace(store, "overview/demo/product", pageOf("overview/demo/product", [rewritten]));
  await waitFor(() => expect(active().textContent).toMatch(/^Overview opens/));
  expect(within(column).getByRole("note")).toBeTruthy();
});
