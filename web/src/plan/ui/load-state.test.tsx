import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import css from "./load-state.css?raw";
import { LoadState } from "./load-state";

afterEach(cleanup);

describe("the state's figure", () => {
  it("is a 40px icon in a 72px disc on a page and 24px in 44px when compact", () => {
    expect(rule(".plan-state")).toMatch(/--plan-state-disc:\s*72px;[^}]*--plan-state-icon:\s*40px/);
    expect(rule('.plan-state[data-size="compact"]')).toMatch(
      /--plan-state-disc:\s*44px;[^}]*--plan-state-icon:\s*24px/,
    );
    expect(rule(".plan-state__glyph")).toMatch(/width:\s*var\(--plan-state-disc\)/);
    expect(rule(".plan-state__glyph > svg")).toMatch(/width:\s*var\(--plan-state-icon\)/);
  });

  it("draws the icon with no fixed size of its own", () => {
    const { container } = render(<LoadState state="empty" />);
    expect(container.querySelector(".plan-state__glyph svg")?.getAttribute("width")).toBe("24");
  });
});

/** The body of the first rule whose selector is exactly `selector`. */
function rule(selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`(?:^|\\n)${escaped}\\s*\\{([^}]*)\\}`).exec(css);
  expect(match, `no rule for ${selector}`).not.toBeNull();
  return match![1]!;
}

describe("LoadState", () => {
  it("draws the spinner, never the roundel, under one status role named by its title", () => {
    const { container } = render(<LoadState state="loading" title="Loading the feature" />);
    const status = screen.getByRole("status", { name: "Loading the feature" });
    expect(screen.getAllByRole("status")).toHaveLength(1);
    expect(status.querySelector("svg.plan-spinner")?.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelector(".aurora-roundel")).toBeNull();
    expect(screen.getByText("Loading the feature")).toBeTruthy();
  });

  it("titles the loading state Loading by default", () => {
    render(<LoadState state="loading" />);
    expect(screen.getByRole("status", { name: "Loading" })).toBeTruthy();
    expect(screen.getByText("Loading")).toBeTruthy();
  });

  it.each([
    ["empty", "Nothing here yet"],
    ["not_found", "Not found"],
    ["error", "Something went wrong"],
  ] as const)("draws %s with a quiet glyph and its default title, no status role", (state, title) => {
    const { container } = render(<LoadState state={state} />);
    const root = container.firstElementChild!;
    expect(root.getAttribute("data-state")).toBe(state);
    expect(root.querySelector(".plan-state__glyph svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(root.querySelector(".plan-spinner")).toBeNull();
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.getByText(title)).toBeTruthy();
  });

  it("draws no dashed card and no hazard stripes", () => {
    for (const state of ["empty", "not_found", "error"] as const) {
      const { container } = render(<LoadState state={state} title="T" message="Details" />);
      expect(container.querySelector(".border-dashed")).toBeNull();
      expect(container.querySelector(".hazard-error")).toBeNull();
      cleanup();
    }
  });

  it("sets the title and the detail on their own lines", () => {
    render(<LoadState state="error" title="Failed" message="Details" />);
    expect(screen.getByText("Failed").className).toBe("plan-state__title");
    expect(screen.getByText("Details").closest(".plan-state__detail")).not.toBeNull();
  });

  it("is the page size by default and compact on request", () => {
    const page = render(<LoadState state="empty" />);
    expect(page.container.firstElementChild?.getAttribute("data-size")).toBe("page");
    cleanup();
    const compact = render(<LoadState state="empty" size="compact" />);
    expect(compact.container.firstElementChild?.getAttribute("data-size")).toBe("compact");
  });

  it.each(["loading", "empty", "not_found", "error"] as const)(
    "shows a bidirectional override in the message of %s as visible text",
    (state) => {
      render(<LoadState state={state} message={"file‮gnp.md"} />);
      const text = screen.getByText("file⟨U+202E⟩gnp.md");
      expect(text.style.unicodeBidi).toBe("isolate");
      expect(document.body.textContent).not.toContain("‮");
    },
  );

  it.each(["loading", "empty", "not_found", "error"] as const)(
    "calls the action for %s",
    (state) => {
      const onClick = vi.fn();
      render(<LoadState state={state} action={{ label: "Continue", onClick }} />);
      fireEvent.click(screen.getByRole("button", { name: "Continue" }));
      expect(onClick).toHaveBeenCalledOnce();
    },
  );

  it("puts its children in the action row after the action", () => {
    render(
      <LoadState state="not_found" action={{ label: "Retry", onClick: () => {} }}>
        <a href="/">Back to home</a>
      </LoadState>,
    );
    const row = screen.getByRole("button", { name: "Retry" }).parentElement!;
    expect(row.className).toBe("plan-state__actions");
    expect(row.lastElementChild?.textContent).toBe("Back to home");
  });

  it("draws no action row without an action or children", () => {
    const { container } = render(<LoadState state="empty" />);
    expect(container.querySelector(".plan-state__actions")).toBeNull();
  });
});

describe("load-state.css", () => {
  it("centres on both axes and fills a flex column's spare height", () => {
    const body = rule(".plan-state");
    expect(body).toMatch(/display:\s*flex/);
    expect(body).toMatch(/flex-direction:\s*column/);
    expect(body).toMatch(/align-items:\s*center/);
    expect(body).toMatch(/justify-content:\s*center/);
    expect(body).toMatch(/flex:\s*1 1 auto/);
    expect(body).toMatch(/text-align:\s*center/);
  });

  it("is at least its parent's height when it is the only child", () => {
    expect(rule(".plan-state:only-child")).toMatch(/min-height:\s*100%/);
  });

  it("drops the loading fade under reduced motion", () => {
    const reduced = /@media \(prefers-reduced-motion: reduce\)\s*\{([\s\S]*)\}\s*$/.exec(css);
    expect(reduced?.[1]).toMatch(/\.plan-state\[data-state="loading"\]\s*\{\s*animation:\s*none/);
  });
});
