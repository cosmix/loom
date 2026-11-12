import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { JSX } from "react";
import { createStore, Provider } from "jotai";
import { MemoryRouter, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import type { FileTree } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";

import { diffBlocks } from "./block-diff";
import { RenderedDiff } from "./rendered-diff";

// jsdom implements no `scrollIntoView`; every test gets a fresh stub on the prototype.
let scrollIntoView = vi.fn();
beforeEach(() => {
  scrollIntoView = vi.fn();
  Object.defineProperty(Element.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: scrollIntoView,
  });
});
afterEach(() => {
  cleanup();
  delete (Element.prototype as Partial<Element>).scrollIntoView;
});

const doc = (blocks: string[]): string => `${blocks.join("\n\n")}\n`;

function renderDiff(before: string, after: string) {
  return render(<RenderedDiff blocks={diffBlocks(before, after)} path="doc/plans/example.md" />);
}

const kinds = (): (string | null)[] =>
  [...document.querySelectorAll("[data-diff-block]")].map((block) =>
    block.getAttribute("data-diff-block"),
  );

/** The sides of a block as `[side, element, spoken label, text]`. */
function sides(block: Element): [string | null, string, string, string][] {
  return [...block.querySelectorAll("[data-diff-side]")].map((side) => [
    side.getAttribute("data-diff-side"),
    side.tagName,
    side.querySelector(".sr-only")?.textContent ?? "",
    side.querySelector(".md")?.textContent ?? "",
  ]);
}

describe("RenderedDiff", () => {
  it("marks each block with its kind and renders its Markdown", () => {
    renderDiff(
      doc(["# Notes", "Stays put.", "Says one thing.", "Goes away."]),
      doc(["# Notes", "Arrives first.", "Stays put.", "Says another thing."]),
    );
    expect(kinds()).toEqual(["same", "added", "same", "changed", "removed"]);
    const [heading, added, , changed, removed] = [
      ...document.querySelectorAll("[data-diff-block]"),
    ];
    // Unchanged text is drawn once, with no mark; the heading is a heading, not `# Notes`.
    expect(heading.querySelector("h1")?.textContent).toBe("Notes");
    expect(heading.querySelector("[data-diff-side]")).toBeNull();
    expect(sides(added)).toEqual([["after", "INS", "Added:", "Arrives first."]]);
    // The first removed block pairs with the added one after it: one changed block, before then
    // after. The second has no partner and stays removed.
    expect(sides(changed)).toEqual([
      ["before", "DEL", "Changed from:", "Says one thing."],
      ["after", "INS", "Changed to:", "Says another thing."],
    ]);
    expect(sides(removed)).toEqual([["before", "DEL", "Removed:", "Goes away."]]);
  });

  it("draws a removed block as a deletion", () => {
    renderDiff(doc(["Stays put.", "Goes away."]), doc(["Stays put."]));
    expect(kinds()).toEqual(["same", "removed"]);
    const removed = document.querySelector('[data-diff-block="removed"]') as Element;
    expect(sides(removed)).toEqual([["before", "DEL", "Removed:", "Goes away."]]);
    expect(removed.querySelector(".diff-side__mark")?.textContent).toBe("−");
    expect(removed.querySelector(".diff-side__mark")?.getAttribute("aria-hidden")).toBe("true");
  });

  it("draws a created file as added blocks only", () => {
    renderDiff("", doc(["# New plan", "Its first paragraph.", "- one\n- two"]));
    expect(kinds()).toEqual(["added", "added", "added"]);
    expect(document.querySelectorAll('[data-diff-side="before"]')).toHaveLength(0);
    expect(document.querySelectorAll("li")).toHaveLength(2);
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("draws a deleted file as removed blocks only", () => {
    renderDiff(doc(["# Old plan", "Its last paragraph."]), "");
    expect(kinds()).toEqual(["removed", "removed"]);
    expect(document.querySelectorAll('[data-diff-side="after"]')).toHaveLength(0);
    expect(document.querySelector("h1")?.textContent).toBe("Old plan");
  });

  it("is a named group the keyboard can focus and scroll", () => {
    renderDiff(doc(["Old."]), doc(["New."]));
    const scroller = screen.getByRole("group", { name: "Rendered changes" });
    expect(scroller.tabIndex).toBe(0);
    expect(scroller.querySelector("[data-diff-block]")).not.toBeNull();
  });

  it("gives headings no id, so the two texts never share one", () => {
    renderDiff(doc(["## Scope", "Old."]), doc(["## Scope", "New."]));
    expect(document.querySelector("h2")?.id).toBe("");
    expect(screen.queryByRole("button", { name: /copy/i })).toBeNull();
  });
});

describe("RenderedDiff folds", () => {
  const unchanged = (from: number, count: number): string[] =>
    Array.from({ length: count }, (_, i) => `Paragraph ${from + i} stays.`);

  it("folds a long unchanged run and keeps two blocks beside each change", () => {
    const before = doc([...unchanged(1, 8), "Old middle.", ...unchanged(9, 10), "Old end."]);
    const after = doc([...unchanged(1, 8), "New middle.", ...unchanged(9, 10), "New end."]);
    renderDiff(before, after);
    // Six of the first eight fold (two lead into the change); of the ten between the changes,
    // two follow the first and two lead into the second.
    const folds = screen.getAllByRole("button");
    expect(folds.map((fold) => fold.textContent)).toEqual([
      "6 unchanged blocks",
      "6 unchanged blocks",
    ]);
    expect(folds.map((fold) => fold.getAttribute("aria-expanded"))).toEqual(["false", "false"]);
    const shown = ["same", "same", "changed", "same", "same", "same", "same", "changed"];
    expect(kinds()).toEqual(shown);
    expect(screen.queryByText("Paragraph 1 stays.")).toBeNull();
    expect(screen.getByText("Paragraph 7 stays.")).toBeTruthy();

    fireEvent.click(folds[0]);
    expect(folds[0].getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByText("Paragraph 1 stays.")).toBeTruthy();
    expect(screen.queryByText("Paragraph 12 stays.")).toBeNull();
    fireEvent.click(folds[0]);
    expect(screen.queryByText("Paragraph 1 stays.")).toBeNull();
  });

  it("leaves a short unchanged run open", () => {
    const before = doc(["Old start.", ...unchanged(1, 7), "Old end."]);
    const after = doc(["New start.", ...unchanged(1, 7), "New end."]);
    renderDiff(before, after);
    // Seven between two changes would hide three, fewer than a fold row is worth.
    expect(screen.queryByRole("button")).toBeNull();
    expect(kinds()).toHaveLength(9);
  });

  it("shows a document with no change whole", () => {
    const text = doc(unchanged(1, 30));
    renderDiff(text, text);
    expect(screen.queryByRole("button")).toBeNull();
    expect(kinds()).toHaveLength(30);
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("closes every fold when the texts change", () => {
    const before = doc([...unchanged(1, 8), "Old end."]);
    const view = renderDiff(before, doc([...unchanged(1, 8), "New end."]));
    fireEvent.click(screen.getByRole("button"));
    expect(screen.getByText("Paragraph 1 stays.")).toBeTruthy();
    view.rerender(
      <RenderedDiff
        blocks={diffBlocks(before, doc([...unchanged(1, 8), "Newer end."]))}
        path="doc/plans/example.md"
      />,
    );
    expect(screen.getByRole("button").getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByText("Paragraph 1 stays.")).toBeNull();
  });
});

describe("RenderedDiff scroll", () => {
  it("centres the first changed block on open and never again", () => {
    const before = doc(["Stays.", "Old one.", "Stays too.", "Old two."]);
    const view = renderDiff(before, doc(["Stays.", "New one.", "Stays too.", "New two."]));
    const first = document.querySelector('[data-diff-block="changed"]');
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
    expect(scrollIntoView.mock.contexts[0]).toBe(first);
    // An instant jump: nothing to animate, so nothing for reduced motion to switch off.
    expect(scrollIntoView).toHaveBeenCalledWith({ block: "center", behavior: "instant" });

    view.rerender(
      <RenderedDiff
        blocks={diffBlocks(before, doc(["Stays.", "Newer one.", "Stays too.", "New two."]))}
        path="doc/plans/example.md"
      />,
    );
    expect(scrollIntoView).toHaveBeenCalledTimes(1);
  });
});

describe("RenderedDiff scroll under a sticky head", () => {
  const blocks = diffBlocks(doc(["Stays.", "Old one."]), doc(["Stays.", "New one."]));
  const HEAD_HEIGHT = 60;

  /** The first block's scroll margins at the moment of the jump, and after it. */
  function marginsAtJump(headPosition: "sticky" | "static"): { during: string[]; after: string[] } {
    const heights = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.classList.contains("diff-head") ? HEAD_HEIGHT : 0;
      });
    const during: string[] = [];
    scrollIntoView.mockImplementation(function (this: HTMLElement) {
      during.push(this.style.scrollMarginTop, this.style.scrollMarginBottom);
    });
    render(
      <div className="diff-view">
        <div className="diff-head" style={{ position: headPosition }} />
        <RenderedDiff blocks={blocks} path="doc/plans/example.md" />
      </div>,
    );
    heights.mockRestore();
    const first = document.querySelector<HTMLElement>('[data-diff-block="changed"]');
    return {
      during,
      after: [first?.style.scrollMarginTop ?? "", first?.style.scrollMarginBottom ?? ""],
    };
  }

  it("centres in what a sticky head leaves, by a top margin of its height and none below", () => {
    const { during, after } = marginsAtJump("sticky");
    expect(during).toEqual([`${HEAD_HEIGHT}px`, "0px"]);
    expect(scrollIntoView).toHaveBeenCalledWith({ block: "center", behavior: "instant" });
    expect(after).toEqual(["", ""]);
  });

  it("centres with no margin where the head does not stick over the text", () => {
    expect(marginsAtJump("static").during).toEqual(["0px", "0px"]);
  });
});

describe("RenderedDiff links", () => {
  const where = (): string => {
    const location = useLocation();
    return `${location.pathname}${location.search}`;
  };
  function Where(): JSX.Element {
    return <output data-testid="where">{where()}</output>;
  }

  function renderLinks() {
    const store = createStore();
    store.set(treeAtom, treeFixture as unknown as FileTree);
    const after = "Read [the notes](../notes.md) and [a gap](./nope.md).\n";
    render(
      <Provider store={store}>
        <MemoryRouter initialEntries={["/start"]}>
          <RenderedDiff blocks={diffBlocks("", after)} path="doc/plans/example.md" />
          <Where />
        </MemoryRouter>
      </Provider>,
    );
  }

  it("checks a link against the tree: a known document is plain, an unknown one is missing", () => {
    renderLinks();
    const notes = screen.getByRole("link", { name: "the notes" });
    expect(notes.getAttribute("title")).toBeNull();
    expect(notes.classList.contains("md-missing")).toBe(false);
    const gap = screen.getByRole("link", { name: "a gap" });
    expect(gap.getAttribute("title")).toBe("not found");
  });

  it("follows a document link through the router", () => {
    renderLinks();
    expect(screen.getByTestId("where").textContent).toBe("/start");
    fireEvent.click(screen.getByRole("link", { name: "the notes" }));
    expect(screen.getByTestId("where").textContent).not.toBe("/start");
    expect(screen.getByTestId("where").textContent).toBe("/files/notes.md");
  });

  it("draws and follows nothing outside a router", () => {
    renderDiff("", "Read [the notes](../notes.md).\n");
    fireEvent.click(screen.getByRole("link", { name: "the notes" }));
    expect(screen.getByRole("link", { name: "the notes" })).toBeTruthy();
  });
});

describe("RenderedDiff reference links", () => {
  function renderReferences(before: string, after: string) {
    const store = createStore();
    store.set(treeAtom, treeFixture as unknown as FileTree);
    render(
      <Provider store={store}>
        <MemoryRouter>
          <RenderedDiff blocks={diffBlocks(before, after)} path="doc/plans/example.md" />
        </MemoryRouter>
      </Provider>,
    );
  }

  it("resolves a reference link in a block against the definition elsewhere in the document", () => {
    renderReferences(
      doc(["Read [the notes][n].", "Stays put.", "[n]: ../notes.md"]),
      doc(["Read [the notes][n] twice.", "Stays put.", "[n]: ../notes.md"]),
    );
    const links = screen.getAllByRole("link", { name: "the notes" });
    expect(links).toHaveLength(2);
    for (const link of links) expect(link.getAttribute("title")).toBeNull();
    expect(document.querySelectorAll("[data-diff-side] .md a")).toHaveLength(2);
  });

  it("resolves each side against its own definitions", () => {
    renderReferences(
      doc(["Read [the notes][n].", "[n]: ../notes.md"]),
      doc(["Read [the notes][n] now.", "[n]: ./nope.md"]),
    );
    const [paragraph] = [...document.querySelectorAll('[data-diff-block="changed"]')];
    const before = paragraph.querySelector('[data-diff-side="before"] a');
    const after = paragraph.querySelector('[data-diff-side="after"] a');
    expect(before?.getAttribute("title")).toBeNull();
    expect(after?.getAttribute("title")).toBe("not found");
  });

  it("draws an unchanged definition as nothing and a changed one as its source", () => {
    renderReferences(
      doc(["Stays put.", "[a]: ../notes.md", "[b]: ./one.md"]),
      doc(["Stays put.", "[a]: ../notes.md", "[b]: ./two.md"]),
    );
    expect(kinds()).toEqual(["same", "changed"]);
    expect(
      [...document.querySelectorAll(".diff-side__definition")].map((code) => code.textContent),
    ).toEqual(["[b]: ./one.md", "[b]: ./two.md"]);
  });
});
