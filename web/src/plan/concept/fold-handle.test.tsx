import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";

import { FoldCap } from "./fold-handle";

afterEach(cleanup);

describe("FoldCap", () => {
  it.each(["start", "end"] as const)("renders the %s cap described by its run", (side) => {
    render(
      <TooltipProvider>
        <FoldCap runId="run-1" side={side} onFold={() => {}} />
      </TooltipProvider>,
    );

    const cap = screen.getByRole("button", { name: "Fold" });
    expect(cap.getAttribute("data-side")).toBe(side);
    expect(cap.getAttribute("aria-describedby")).toBe("run-1");
    expect(cap.getAttribute("tabindex")).toBe("-1");
  });

  it.each(["start", "end"] as const)("clicking the %s cap folds once without bubbling", (side) => {
    const onFold = vi.fn();
    const onParentClick = vi.fn();
    render(
      <TooltipProvider>
        <div onClick={onParentClick}>
          <FoldCap runId="run-1" side={side} onFold={onFold} />
        </div>
      </TooltipProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Fold" }));

    expect(onFold).toHaveBeenCalledTimes(1);
    expect(onParentClick).not.toHaveBeenCalled();
  });

  it.each(["start", "end"] as const)(
    "focusing the %s cap shows the fold shortcut",
    async (side) => {
      render(
        <TooltipProvider>
          <FoldCap runId="run-1" side={side} onFold={() => {}} />
        </TooltipProvider>,
      );
      const cap = screen.getByRole("button", { name: "Fold" });

      fireEvent.focus(cap);

      expect((await screen.findByRole("tooltip")).textContent).toBe("Fold · Esc");
      expect(screen.getByRole("button", { name: "Fold" })).toBe(cap);
      expect(cap.getAttribute("aria-describedby")).toBe("run-1");
    },
  );
});
