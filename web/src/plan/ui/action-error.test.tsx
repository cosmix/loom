import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import { ActionError } from "./action-error";

afterEach(cleanup);

it("announces an action error as an alert with its format characters visible", () => {
  render(<ActionError message={"Could not delete loom/‮abc"} />);
  expect(screen.getByRole("alert").textContent).toBe("Could not delete loom/⟨U+202E⟩abc");
});

it("renders markup in the message as text and wraps a long unbroken token", () => {
  const { container } = render(<ActionError message="<img src=x onerror=alert(1)>" />);
  const alert = screen.getByRole("alert");
  expect(alert.textContent).toBe("<img src=x onerror=alert(1)>");
  expect(container.querySelector("img")).toBeNull();
  expect(alert.classList.contains("[overflow-wrap:anywhere]")).toBe(true);
});

it("leads the message with its title, and keeps the message its own element", () => {
  render(<ActionError title="Could not mark all seen" message="Hashes are unavailable" />);
  expect(screen.getByRole("alert").textContent).toBe(
    "Could not mark all seen: Hashes are unavailable",
  );
  expect(screen.getByText("Hashes are unavailable").tagName).toBe("SPAN");
});
