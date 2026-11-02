import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { LoadState } from "./load-state";

afterEach(cleanup);

describe("LoadState", () => {
  it("renders a skeleton while loading", () => {
    const { container } = render(<LoadState state="loading" />);
    expect(screen.getByRole("status").getAttribute("aria-label")).toBe("Loading");
    expect(container.querySelector('[data-slot="skeleton"]')).not.toBeNull();
  });

  it.each(["empty", "not_found"] as const)("renders an empty state for %s", (state) => {
    const { container } = render(
      <LoadState state={state} title="Custom title" message="Details" />,
    );
    expect(screen.getByText("Custom title")).toBeTruthy();
    expect(screen.getByText("Details")).toBeTruthy();
    expect(container.querySelector(".border-dashed")).not.toBeNull();
  });

  it("renders an error hazard panel", () => {
    const { container } = render(<LoadState state="error" title="Failed" message="Details" />);
    expect(screen.getByText("Failed")).toBeTruthy();
    expect(screen.getByText("Details")).toBeTruthy();
    expect(container.querySelector(".hazard-error")).not.toBeNull();
  });

  it.each(["loading", "empty", "not_found", "error"] as const)(
    "shows a bidirectional override in the message of %s as visible text",
    (state) => {
      render(<LoadState state={state} message={"file\u202egnp.md"} />);
      const text = screen.getByText("file⟨U+202E⟩gnp.md");
      expect(text.style.unicodeBidi).toBe("isolate");
      expect(document.body.textContent).not.toContain("\u202e");
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
});
