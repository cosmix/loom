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
    // Button dims only the `disabled` attribute, so a blocked control styles `aria-disabled` itself.
    expect(button.classList.contains("aria-disabled:opacity-50")).toBe(true);
    expect(button.classList.contains("aria-disabled:cursor-not-allowed")).toBe(true);
    // The reason's text sits in the isolated `Untrusted` span inside the styled one.
    expect(screen.getByText("Dana holds authoring").parentElement?.className).toContain(
      "text-muted-foreground",
    );
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

  it("shows a bidi control in the reason as a visible code point, isolated from its neighbours", () => {
    render(
      <DisabledReason reason={"Dana\u202e holds authoring"} onClick={vi.fn()}>
        Write
      </DisabledReason>,
    );

    const reason = screen.getByText("Dana⟨U+202E⟩ holds authoring");
    expect(reason.textContent).not.toContain("\u202e");
    expect(reason.style.unicodeBidi).toBe("isolate");
    expect(screen.getByRole("button", { name: "Write" }).getAttribute("aria-describedby")).toBe(
      reason.parentElement?.id,
    );
  });

  it("draws the design-system button, secondary by default, primary only when asked", () => {
    render(
      <>
        <DisabledReason reason={null} onClick={vi.fn()}>
          Start
        </DisabledReason>
        <DisabledReason reason={null} look="primary" onClick={vi.fn()}>
          Begin
        </DisabledReason>
        <DisabledReason reason={null} size="sm" onClick={vi.fn()}>
          Move
        </DisabledReason>
      </>,
    );

    const plain = screen.getByRole("button", { name: "Start" });
    expect(plain.classList.contains("plan-button")).toBe(true);
    expect(plain.getAttribute("data-look")).toBe("secondary");
    expect(screen.getByRole("button", { name: "Begin" }).getAttribute("data-look")).toBe("primary");
    const sized = screen.getByRole("button", { name: "Move" });
    expect(sized.classList.contains("plan-button")).toBe(true);
    expect(sized.classList.contains("h-7")).toBe(true);
    expect(sized.getAttribute("data-look")).toBe("secondary");
    expect(sized.getAttribute("data-size")).toBe("sm");
  });
});
