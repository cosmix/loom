import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { LockNotice } from "@/plan/files/lock-notice";

import { DocHeading, type DocHeadingProps } from "./doc-heading";
import headingSheet from "@/plan/ui/doc-heading.css?raw";

const clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  if (clipboardDescriptor) {
    Object.defineProperty(navigator, "clipboard", clipboardDescriptor);
  } else {
    Reflect.deleteProperty(navigator, "clipboard");
  }
});

function renderHeading(props: Partial<DocHeadingProps> = {}) {
  return render(<DocHeading breadcrumb={[]} kind="plan" title="Execution handoff" {...props} />, {
    wrapper: MemoryRouter,
  });
}

function stubClipboard(writeText = vi.fn().mockResolvedValue(undefined)) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
  return writeText;
}

it("renders breadcrumb links and identifies the last crumb as the current page", () => {
  renderHeading({
    breadcrumb: [
      { label: "Planner UI", to: "/" },
      { label: "Plans", to: "/plans" },
      { label: "feat-planner-11" },
    ],
  });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(within(nav).getByRole("link", { name: "Planner UI" }).getAttribute("href")).toBe("/");
  expect(within(nav).getByRole("link", { name: "Plans" }).getAttribute("href")).toBe("/plans");
  expect(
    within(nav)
      .getByText("feat-planner-11")
      .closest("[aria-current]")
      ?.getAttribute("aria-current"),
  ).toBe("page");
  expect(nav.querySelectorAll('[aria-current="page"]')).toHaveLength(1);
  expect(nav.querySelectorAll('svg[aria-hidden="true"]')).toHaveLength(2);
});

it("marks a linked final breadcrumb as the current page", () => {
  renderHeading({ breadcrumb: [{ label: "Plans", to: "/plans" }] });

  expect(screen.getByRole("link", { name: "Plans" }).getAttribute("aria-current")).toBe("page");
});

it("renders the document title as the main heading", () => {
  renderHeading();

  const title = screen.getByRole("heading", { level: 1, name: "Execution handoff" });
  expect(title.classList.contains("plan-page-title")).toBe(true);
  expect(title.classList.contains("font-[650]")).toBe(true);
  expect(title.classList.contains("text-balance")).toBe(true);
  expect(title.closest("header")).not.toBeNull();
});

it("renders server titles and paths with visible controls and isolated text", () => {
  const { container } = renderHeading({ title: "Plan\u202Etitle", path: "doc/\u2066plan.md" });

  const title = screen.getByRole("heading", { name: "Plan⟨U+202E⟩title" });
  const path = screen.getByText("doc/⟨U+2066⟩plan.md");
  expect((title.firstElementChild as HTMLElement).style.unicodeBidi).toBe("isolate");
  expect(path.style.direction).toBe("ltr");
  expect(container.querySelector("code")?.textContent).toBe("doc/⟨U+2066⟩plan.md");
});

it.each(["summary", "source"] as const)(
  "marks the %s view current and keeps available views linked",
  (current) => {
    renderHeading({ views: { current, summaryTo: "/summary", sourceTo: "/source" } });

    const group = screen.getByRole("group", { name: "View" });
    const summary = within(group).getByRole("link", { name: "Summary" });
    const source = within(group).getByRole("link", { name: "Source" });
    expect(summary.getAttribute("href")).toBe("/summary");
    expect(source.getAttribute("href")).toBe("/source");
    expect(summary.getAttribute("aria-current")).toBe(current === "summary" ? "page" : null);
    expect(source.getAttribute("aria-current")).toBe(current === "source" ? "page" : null);
  },
);

it.each([
  ["summary", "Source"],
  ["source", "Summary"],
] as const)("disables the unavailable other view when %s is current", (current, disabledLabel) => {
  renderHeading({ views: { current, summaryTo: null, sourceTo: null } });

  const group = screen.getByRole("group", { name: "View" });
  const disabled = within(group).getByText(disabledLabel);
  const active = group.querySelector('[aria-current="page"]');
  expect(disabled.tagName).toBe("SPAN");
  expect(disabled.getAttribute("aria-disabled")).toBe("true");
  expect(active?.getAttribute("aria-disabled")).toBeNull();
  expect(active?.getAttribute("data-look")).toBe("quiet");
  expect(within(group).queryAllByRole("link")).toHaveLength(0);
});

it("draws the views as one segmented control inside the bordered toolbar bar, the current one filled", () => {
  renderHeading({ views: { current: "source", summaryTo: "/summary", sourceTo: "/source" } });

  const group = screen.getByRole("group", { name: "View" });
  expect(group.className).toBe("doc-heading-views");
  expect(group.parentElement?.className).toBe("doc-heading-group");
  expect(group.parentElement?.parentElement?.className).toBe("doc-heading-toolbar");
  const css = headingSheet.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
  // The bar carries the one edge; the segments sit flush inside its first group.
  expect(css).toContain(
    ".doc-heading-toolbar { display: flex; flex-wrap: wrap; align-items: stretch; min-width: 0; border: 1px solid var(--border);",
  );
  expect(css).toContain(
    ".doc-heading-views { display: inline-flex; flex: none; align-items: stretch; gap: 2px; height: var(--control-height-sm); }",
  );
  expect(css).toContain(
    ".doc-heading-group + .doc-heading-group { border-inline-start: 1px solid var(--border); }",
  );
  expect(css).toContain(
    '.doc-heading-views > .plan-button[aria-current="page"] { background: var(--muted); color: var(--foreground);',
  );
  // Hovering the other segment never gives it the current segment's fill.
  expect(css).toContain(".doc-heading-views > .plan-button:hover { background: transparent; }");
});

it("renders actions after the view switch", () => {
  renderHeading({
    views: { current: "summary", summaryTo: "/summary", sourceTo: "/source" },
    actions: <button type="button">Discuss</button>,
  });

  const views = screen.getByRole("group", { name: "View" }).parentElement as HTMLElement;
  expect(views.className).toBe("doc-heading-group");
  expect(views.nextElementSibling).toBe(screen.getByRole("button", { name: "Discuss" }));
});

it("omits optional view, chip, path and lock rows when their data is absent", () => {
  const { container } = renderHeading({ chips: [], path: null });

  expect(screen.queryByRole("group", { name: "View" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Copy path" })).toBeNull();
  expect(screen.queryByRole("note")).toBeNull();
  expect(container.querySelector("header")?.children).toHaveLength(2);
});

it("renders linked, pressed-button and plain chips as rows of one status card", () => {
  const onClick = vi.fn();
  renderHeading({
    chips: [
      { key: "implements", label: "Implements §11", to: "/product/11", title: "Product section" },
      {
        key: "filter",
        label: "Running",
        onClick,
        pressed: true,
        icon: <svg data-testid="chip-icon" />,
      },
      { key: "valid", label: "Valid", title: "No warnings", tone: "ok" },
    ],
  });

  const link = screen.getByRole("link", { name: "Implements §11" });
  const button = screen.getByRole("button", { name: "Running" });
  const plain = screen.getByTitle("No warnings");
  expect(link.getAttribute("href")).toBe("/product/11");
  expect(link.getAttribute("title")).toBe("Product section");
  expect(button.getAttribute("type")).toBe("button");
  expect(button.getAttribute("aria-pressed")).toBe("true");
  expect(screen.getByTestId("chip-icon").parentElement?.getAttribute("aria-hidden")).toBe("true");
  expect(plain.tagName).toBe("SPAN");
  const rows = [link, button, plain].map((chip) => chip.parentElement as HTMLElement);
  for (const row of rows) expect(row.className).toBe("doc-heading-row");
  const card = screen.getByRole("list", { name: "Status" });
  for (const row of rows) expect(row.parentElement).toBe(card);
  expect(card.classList.contains("plan-surface")).toBe(true);
  // The glyph column comes first on every row, drawn empty where the chip has no glyph.
  expect(link.firstElementChild?.className).toContain("doc-heading-chip__glyph");
  expect(link.firstElementChild?.childElementCount).toBe(0);

  fireEvent.click(button);
  expect(onClick).toHaveBeenCalledOnce();
});

it.each([false, undefined])("preserves a chip button's pressed=%s state", (pressed) => {
  renderHeading({ chips: [{ key: "toggle", label: "Filter", onClick: vi.fn(), pressed }] });

  expect(screen.getByRole("button", { name: "Filter" }).getAttribute("aria-pressed")).toBe(
    pressed === false ? "false" : null,
  );
});

it("includes the missing-target explanation in a plain chip's readable text", () => {
  renderHeading({
    chips: [
      { key: "missing", label: "Implements §3", title: "not found", description: "not found" },
    ],
  });

  const chip = screen.getByTitle("not found");
  expect(chip.textContent).toBe("Implements §3 (not found)");
  expect(chip.querySelector(".sr-only")?.textContent).toBe("(not found)");
  expect(chip.closest("a, button")).toBeNull();
});

it.each(["link", "button"] as const)(
  "includes chip descriptions in the %s accessible name",
  (role) => {
    renderHeading({
      chips: [
        {
          key: "section",
          label: "Implements §3",
          description: "not found",
          ...(role === "link" ? { to: "/product/3" } : { onClick: vi.fn() }),
        },
      ],
    });

    expect(screen.getByRole(role, { name: "Implements §3 (not found)" })).toBeTruthy();
  },
);

it("copies the original path and announces success for 1.5 seconds", async () => {
  vi.useFakeTimers();
  const writeText = stubClipboard();
  const { container } = renderHeading({ path: "doc/\u2066plan.md" });
  const live = container.querySelector('[role="status"]');
  expect(live?.textContent).toBe("");

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Copy path" }));
  });

  expect(writeText).toHaveBeenCalledWith("doc/\u2066plan.md");
  expect(screen.getByText("Copied")).toBe(live);
  act(() => {
    vi.advanceTimersByTime(1499);
  });
  expect(live?.textContent).toBe("Copied");
  act(() => {
    vi.advanceTimersByTime(1);
  });
  expect(live?.textContent).toBe("");
});

it("clears the copy feedback timer on unmount", async () => {
  vi.useFakeTimers();
  stubClipboard();
  const { unmount } = renderHeading({ path: "doc/plan.md" });

  await act(async () => {
    fireEvent.click(screen.getByRole("button", { name: "Copy path" }));
  });
  // The copy button's own reset timer plus the heading's announcement timer.
  expect(vi.getTimerCount()).toBe(2);
  unmount();

  expect(vi.getTimerCount()).toBe(0);
});

it("does not schedule feedback if copying finishes after unmount", async () => {
  vi.useFakeTimers();
  let resolveCopy!: () => void;
  stubClipboard(
    vi.fn().mockReturnValue(
      new Promise<void>((resolve) => {
        resolveCopy = resolve;
      }),
    ),
  );
  const { unmount } = renderHeading({ path: "doc/plan.md" });
  fireEvent.click(screen.getByRole("button", { name: "Copy path" }));
  unmount();

  await act(async () => {
    resolveCopy();
  });

  expect(vi.getTimerCount()).toBe(0);
});

it("handles clipboard rejection without announcing success", async () => {
  stubClipboard(vi.fn().mockRejectedValue(new Error("Permission denied")));
  renderHeading({ path: "doc/plan.md" });

  fireEvent.click(screen.getByRole("button", { name: "Copy path" }));

  expect(await screen.findByText("Copy failed")).not.toBeNull();
  expect(screen.queryByText("Copied")).toBeNull();
});

it("handles a missing clipboard API and selects the path for a manual copy", async () => {
  Reflect.deleteProperty(navigator, "clipboard");
  const { container } = renderHeading({ path: "doc/plan.md" });

  fireEvent.click(screen.getByRole("button", { name: "Copy path" }));

  expect(await screen.findByText("Copy failed", { selector: '[role="status"]' })).not.toBeNull();
  expect(window.getSelection()?.toString()).toBe(container.querySelector("code")?.textContent);
});

it("announces through role=status without an aria-live attribute", () => {
  const { container } = renderHeading({ path: "doc/plan.md" });

  expect(container.querySelector("[aria-live]")).toBeNull();
  expect(container.querySelector('[role="status"]')).not.toBeNull();
});

it("links uncommitted changes at the end of the path row, after a warning dot outside the link", () => {
  const { container } = renderHeading({
    path: "doc/plan.md",
    changes: { to: "/diff/head/doc/plan.md" },
  });

  const link = screen.getByRole("link", { name: "Uncommitted changes" });
  expect(link.getAttribute("href")).toBe("/diff/head/doc/plan.md");
  const pair = link.parentElement as HTMLElement;
  expect(pair.className).toContain("ml-auto");
  expect(pair.parentElement?.querySelector("code")?.textContent).toBe("doc/plan.md");
  expect(link.querySelector("[aria-hidden]")).toBeNull();
  expect(link.textContent).toBe("Uncommitted changes");
  const dot = link.previousElementSibling as HTMLElement;
  expect(dot.tagName).toBe("SPAN");
  expect(dot.getAttribute("aria-hidden")).toBe("true");
  expect(dot.parentElement).toBe(pair);
  expect(dot.className).toContain("size-[7px]");
  expect(dot.className).toContain("bg-(--tone-warning)");
  expect(container.querySelector("svg.lucide-file")).not.toBeNull();
});

it("renders the lock notice and leaves children last inside the header", () => {
  const { container } = renderHeading({
    children: (
      <>
        <LockNotice notice="This plan is running." />
        <ul aria-label="Validation">
          <li>All checks passed</li>
        </ul>
      </>
    ),
  });

  const note = screen.getByRole("note");
  expect(note.textContent).toBe("This plan is running.");
  expect(note.querySelector('svg[aria-hidden="true"]')).not.toBeNull();
  for (const name of ["bg-muted", "rounded-[8px]", "gap-2", "px-2.5", "py-2", "text-[12.5px]"]) {
    expect(note.classList.contains(name)).toBe(true);
  }
  expect(container.querySelector("header")?.lastElementChild).toBe(
    screen.getByRole("list", { name: "Validation" }),
  );
});

it("passes kind, large size and rebuilding to the document mark", () => {
  const { container } = renderHeading({ kind: "vision", rebuilding: true });

  const activity = container.querySelector('[data-kind="summaries"]');
  expect(activity?.parentElement?.classList.contains("size-[30px]")).toBe(true);
  expect(activity?.parentElement?.style.color).toBe("var(--doc-1)");
});

it("renders no document mark and leaves the title alone in its group when kind is null", () => {
  const { container } = renderHeading({ kind: null, rebuilding: true });

  const group = screen.getByRole("heading", { level: 1 }).parentElement;
  expect(group?.querySelector("svg")).toBeNull();
  expect(container.querySelector('[data-kind="summaries"]')).toBeNull();
  expect(group?.children).toHaveLength(1);
  expect(group?.parentElement?.children).toHaveLength(2);
});

it("keeps the title row to the mark and the title, with the actions in the toolbar under it", () => {
  renderHeading({ actions: <button type="button">Edit</button> });

  const row = screen.getByRole("heading", { level: 1 }).parentElement as HTMLElement;
  expect(row.className).toBe("doc-heading-title-row");
  expect(row.children).toHaveLength(2);
  expect(row.querySelector("button")).toBeNull();
  const toolbar = screen.getByRole("button", { name: "Edit" }).parentElement as HTMLElement;
  expect(toolbar.className).toBe("doc-heading-toolbar");
  expect(toolbar.previousElementSibling).toBe(row);
});
