import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Provider } from "jotai/react";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { store } from "@/state/store";

import { DiffView } from "./diff-view";

// The real CodeMirror chunk through the real loader, no `@/plan/editor/load` mock.

beforeAll(() => {
  // jsdom has no text layout, and CodeMirror measures through Range rectangles.
  Range.prototype.getBoundingClientRect = () => new DOMRect(0, 0, 0, 0);
  Range.prototype.getClientRects = () => [] as unknown as DOMRectList;
});

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

function renderDiff(before: string, after: string): void {
  render(
    <Provider store={store}>
      <TooltipProvider>
        <DiffView before={before} after={after} path="doc/plans/example.md" />
      </TooltipProvider>
    </Provider>,
  );
}

/** Every text node in the document, so markdown source shown as text cannot hide. */
function textNodes(): string[] {
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  const texts: string[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    texts.push(node.textContent ?? "");
  }
  return texts;
}

it("DiffView opens on the rendered markdown and switches to the source view", async () => {
  renderDiff("## Old title\n", "## New title\n\nA paragraph under the new title.\n");

  await waitFor(() => {
    const headings = [...document.querySelectorAll("h2")];
    expect(headings.some((h) => h.textContent?.includes("New title"))).toBe(true);
  });
  expect(textNodes().filter((text) => text.includes("## "))).toEqual([]);
  expect(document.querySelector('[data-diff-block="changed"]')).not.toBeNull();
  expect(document.querySelector(".cm-editor")).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Source" }));
  await waitFor(() => expect(document.querySelector(".cm-editor")).not.toBeNull(), {
    timeout: 5000,
  });
  expect(screen.getByRole("button", { name: "Rendered" })).toBeTruthy();
});

it("DiffView scrolls the first changed block to the centre on open", async () => {
  const unchanged = Array.from({ length: 20 }, (_, i) => `Paragraph ${i + 1} stays the same.`);
  const before = `${[...unchanged, "The closing paragraph says one thing."].join("\n\n")}\n`;
  const after = `${[...unchanged, "The closing paragraph says another."].join("\n\n")}\n`;
  renderDiff(before, after);

  await waitFor(() => expect(scrollIntoView).toHaveBeenCalled());
  const changed = document.querySelector('[data-diff-block="changed"]');
  expect(changed).not.toBeNull();
  const onChanged = scrollIntoView.mock.calls.filter(
    (_, i) => scrollIntoView.mock.contexts[i] === changed,
  );
  expect(onChanged).toContainEqual([expect.objectContaining({ block: "center" })]);
});
