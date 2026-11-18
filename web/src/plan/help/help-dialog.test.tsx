import { readFileSync } from "node:fs";
import { join } from "node:path";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { useState, type ReactNode } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { PlannerHelpDialog } from "@/plan/help/help-dialog";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";

vi.mock("@/plan/help/topics", async () => {
  const { BookOpenIcon, CompassIcon } = await import("lucide-react");
  return {
    PLANNER_HELP_TOPICS: [
      {
        id: "around",
        label: "Getting around",
        description: "Planner navigation",
        Icon: CompassIcon,
        hue: 1,
        intro: "Find your way through the planner.",
        scenes: [
          {
            title: "The planner header",
            notes: [],
            scale: 0.74,
            Scene: () => (
              <div>
                Header preview
                <textarea aria-label="Example text" />
              </div>
            ),
          },
        ],
        tips: ["Your work is saved as you go."],
      },
      {
        id: "reading",
        label: "Reading summaries",
        description: "Passage guide",
        Icon: BookOpenIcon,
        hue: 2,
        intro: "Follow excerpts back to the source.",
        scenes: [],
        Body: () => <p>Reading details</p>,
      },
    ],
  };
});

vi.mock("@/plan/help/pinned-scene", () => ({
  PinnedScene: ({
    number,
    title,
    children,
  }: {
    number: number;
    title: string;
    children: ReactNode;
  }) => (
    <section>
      <h3>
        Scene {number}: {title}
      </h3>
      {children}
    </section>
  ),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function show(onOpenChange = vi.fn()) {
  render(<PlannerHelpDialog open onOpenChange={onOpenChange} />);
  return {
    onOpenChange,
    search: screen.getByRole<HTMLInputElement>("searchbox", { name: "Search help" }),
  };
}

function topicNav() {
  return within(screen.getByRole("navigation", { name: "Help topics" }));
}

function HelpLauncher() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open help
      </button>
      <PlannerHelpDialog open={open} onOpenChange={setOpen} />
    </>
  );
}

it("renders the dialog title, first topic, scene and tips", () => {
  show();

  expect(screen.getByRole("dialog", { name: "Help" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Getting around" })).toBeTruthy();
  expect(screen.getByText("Topic 1 of 2")).toBeTruthy();
  expect(screen.getByText("Find your way through the planner.")).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Scene 1: The planner header" })).toBeTruthy();
  expect(screen.getByText("Header preview")).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Good to know" })).toBeTruthy();
  expect(screen.getByText("Your work is saved as you go.")).toBeTruthy();
  expect(screen.getByText("?").tagName).toBe("KBD");
  expect(screen.getByText("Esc").tagName).toBe("KBD");
  expect(screen.getByText(/opens this guide/)).toBeTruthy();
  expect(screen.getByPlaceholderText("Search help")).toBeTruthy();
});

it("applies reading preferences to the portalled dialog content", () => {
  const store = createStore();
  store.set(readingFaceAtom, "literata");
  store.set(readingSizeAtom, 21);

  render(
    <Provider store={store}>
      <PlannerHelpDialog open onOpenChange={vi.fn()} />
    </Provider>,
  );

  const dialog = screen.getByRole("dialog", { name: "Help" });
  expect(dialog.hasAttribute("data-loom-plan")).toBe(true);
  expect(dialog.getAttribute("data-reading-face")).toBe("literata");
  expect(dialog.getAttribute("data-reading-size")).toBe("21");
});

it("switches topics through nav buttons and marks only the current topic", () => {
  show();
  const first = topicNav().getByRole("button", { name: /Getting around/ });
  const second = topicNav().getByRole("button", { name: /Reading summaries/ });
  expect(first.getAttribute("aria-current")).toBe("page");
  expect(second.hasAttribute("aria-current")).toBe(false);

  fireEvent.click(second);

  expect(screen.getByRole("heading", { name: "Reading summaries" })).toBeTruthy();
  expect(screen.getByText("Topic 2 of 2")).toBeTruthy();
  expect(screen.getByText("Follow excerpts back to the source.")).toBeTruthy();
  expect(screen.getByText("Reading details")).toBeTruthy();
  expect(second.getAttribute("aria-current")).toBe("page");
  expect(first.hasAttribute("aria-current")).toBe(false);
  expect(screen.queryByText("Header preview")).toBeNull();
});

it.each(["rEaDiNg", "PASSAGE GUIDE", "excerpts"])(
  "matches labels, descriptions and intros case-insensitively: %s",
  (query) => {
    const { search } = show();

    fireEvent.change(search, { target: { value: query } });

    expect(topicNav().getAllByRole("button")).toHaveLength(1);
    expect(
      topicNav()
        .getByRole("button", { name: /Reading summaries/ })
        .getAttribute("aria-current"),
    ).toBe("page");
    expect(screen.getByRole("heading", { name: "Reading summaries" })).toBeTruthy();
    expect(screen.getByText("Topic 2 of 2")).toBeTruthy();
  },
);

it("shows the empty state and restores topics with Clear search", () => {
  const { search } = show();

  fireEvent.change(search, { target: { value: "nothing matches" } });

  expect(topicNav().queryAllByRole("button")).toHaveLength(0);
  expect(screen.getByRole("status").textContent).toBe("No topics match your search.");
  expect(screen.queryByRole("article")).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Clear search" }));

  expect(search.value).toBe("");
  expect(topicNav().getAllByRole("button")).toHaveLength(2);
  expect(screen.getByRole("heading", { name: "Getting around" })).toBeTruthy();
});

it("focuses search with / outside text fields", () => {
  const { search } = show();
  const button = topicNav().getByRole("button", { name: /Getting around/ });
  button.focus();

  fireEvent.keyDown(button, { key: "/" });

  expect(document.activeElement).toBe(search);
});

it("leaves / available to a text field", () => {
  const { search } = show();
  const field = screen.getByRole("textbox", { name: "Example text" });
  field.focus();

  fireEvent.keyDown(field, { key: "/" });

  expect(document.activeElement).toBe(field);
  expect(document.activeElement).not.toBe(search);
});

it("clears a focused search on Escape before closing the dialog", () => {
  const { search, onOpenChange } = show();
  search.focus();
  fireEvent.change(search, { target: { value: "missing" } });

  fireEvent.keyDown(search, { key: "Escape" });

  expect(search.value).toBe("");
  expect(onOpenChange).not.toHaveBeenCalled();
  expect(screen.getByRole("dialog", { name: "Help" })).toBeTruthy();

  fireEvent.keyDown(search, { key: "Escape" });

  expect(onOpenChange).toHaveBeenCalledWith(false);
});

it("moves with next and previous cards and hides unavailable ends", () => {
  show();
  expect(screen.queryByRole("button", { name: /^Previous topic:/ })).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Next topic: Reading summaries" }));

  expect(screen.getByRole("heading", { name: "Reading summaries" })).toBeTruthy();
  expect(screen.queryByRole("button", { name: /^Next topic:/ })).toBeNull();

  fireEvent.click(screen.getByRole("button", { name: "Previous topic: Getting around" }));

  expect(screen.getByRole("heading", { name: "Getting around" })).toBeTruthy();
  expect(
    topicNav()
      .getByRole("button", { name: /Getting around/ })
      .getAttribute("aria-current"),
  ).toBe("page");
});

it("lets the pager leave the current search results", () => {
  const { search } = show();
  fireEvent.change(search, { target: { value: "planner" } });

  fireEvent.click(screen.getByRole("button", { name: "Next topic: Reading summaries" }));

  expect(search.value).toBe("");
  expect(screen.getByText("Topic 2 of 2")).toBeTruthy();
  expect(topicNav().getAllByRole("button")).toHaveLength(2);
});

it("resets article scrolling when selecting another topic", () => {
  show();
  const article = screen.getByRole("article");
  article.scrollTop = 180;

  fireEvent.click(topicNav().getByRole("button", { name: /Reading summaries/ }));

  expect(article.scrollTop).toBe(0);
});

function phone() {
  vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
    matches: query === "(max-width: 767px)",
    media: query,
    onchange: null,
    addListener() {},
    removeListener() {},
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent: () => true,
  }));
  const scrollIntoView = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    value: scrollIntoView,
  });
  return scrollIntoView;
}

afterEach(() => {
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView");
});

// The dialog scrolls as a whole on a phone: the list, then the current topic below it.
it("shows the topic list and the first topic together on phones", () => {
  phone();
  show();

  expect(topicNav().getAllByRole("button")).toHaveLength(2);
  expect(screen.getByRole("article")).toBeTruthy();
  expect(screen.getByText("Topic 1 of 2")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "All topics" })).toBeNull();
});

it("scrolls a chosen topic into view on phones and focuses its article", () => {
  const scrollIntoView = phone();
  show();
  expect(scrollIntoView).not.toHaveBeenCalled();

  fireEvent.click(topicNav().getByRole("button", { name: /Reading summaries/ }));

  const article = screen.getByRole("article");
  expect(article).toBe(document.activeElement);
  expect(scrollIntoView).toHaveBeenCalledTimes(1);
  expect(scrollIntoView.mock.contexts[0]).toBe(article);
  expect(screen.getByText("Topic 2 of 2")).toBeTruthy();
  expect(topicNav().getAllByRole("button")).toHaveLength(2);
});

it("does not scroll or take focus while a phone reader types a search", () => {
  const scrollIntoView = phone();
  const { search } = show();
  search.focus();

  fireEvent.change(search, { target: { value: "passage" } });

  expect(screen.getByText("Topic 2 of 2")).toBeTruthy();
  expect(scrollIntoView).not.toHaveBeenCalled();
  expect(document.activeElement).toBe(search);
});

it("keeps the dialog's phone styles scrolling as a whole", () => {
  const css = readFileSync(join(import.meta.dirname, "help-dialog.css"), "utf8");
  const phoneRules = css.slice(css.indexOf("@media (max-width: 767px)"));

  expect(phoneRules).toMatch(/\.planner-help\s*\{[^}]*overflow-y:\s*auto;/);
  expect(phoneRules).toMatch(/\.planner-help-article\s*\{[^}]*overflow:\s*visible;/);
  expect(phoneRules).not.toContain("grid-template-rows");
});

it("keeps Close in reach on a phone while the dialog scrolls", () => {
  const css = readFileSync(join(import.meta.dirname, "help-dialog.css"), "utf8");
  const phoneRules = css.slice(css.indexOf("@media (max-width: 767px)"));
  const close = phoneRules.match(/\[data-slot="dialog-close"\]\s*\{([^}]*)\}/)?.[1] ?? "";

  expect(close).toMatch(/position:\s*sticky;/);
  expect(close).toMatch(/top:\s*8px;/);
  expect(close).toMatch(/order:\s*-1;/);
  expect(close).toMatch(/background:\s*var\(--background\);/);
  expect(phoneRules).toMatch(/\.planner-help\s*\{[^}]*flex-direction:\s*column;/);
});

it("returns focus to its opener without a Radix trigger", async () => {
  render(<HelpLauncher />);
  const opener = screen.getByRole("button", { name: "Open help" });
  opener.focus();
  fireEvent.click(opener);
  expect(screen.getByRole("dialog").contains(document.activeElement)).toBe(true);

  fireEvent.click(screen.getByRole("button", { name: "Close" }));

  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  await waitFor(() => expect(document.activeElement).toBe(opener));
});
