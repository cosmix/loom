import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DisabledReason } from "./disabled-reason";

afterEach(cleanup);

describe("DisabledReason", () => {
  it("keeps a blocked button focusable and describes why it cannot be used", () => {
    const onClick = vi.fn();
    render(
      <DisabledReason reason="Dana holds authoring" onClick={onClick}>
        Write
      </DisabledReason>,
    );

    const button = screen.getByRole("button", {
      name: "Write",
      description: "Dana holds authoring",
    });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    expect(button.hasAttribute("disabled")).toBe(false);
    expect(screen.getByText("Dana holds authoring").className).toContain("text-muted-foreground");
    button.focus();
    expect(document.activeElement).toBe(button);
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("calls the handler when enabled and updates when the reason changes", () => {
    const onClick = vi.fn();
    const { rerender } = render(
      <DisabledReason reason="Locked" onClick={onClick}>
        Write
      </DisabledReason>,
    );
    rerender(
      <DisabledReason reason={null} onClick={onClick}>
        Write
      </DisabledReason>,
    );

    const button = screen.getByRole("button", { name: "Write" });
    expect(button.hasAttribute("aria-disabled")).toBe(false);
    expect(button.hasAttribute("aria-describedby")).toBe(false);
    expect(screen.queryByText("Locked")).toBeNull();
    fireEvent.click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });
});
