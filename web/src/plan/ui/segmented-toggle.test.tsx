import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";

import { SegmentedToggle } from "./segmented-toggle";

afterEach(cleanup);

const OPTIONS = [
  { value: "one", label: "First view" },
  { value: "two", label: "Second view" },
] as const;

it("is a named group of buttons, the chosen one pressed, each named by its label", () => {
  render(<SegmentedToggle label="View" options={OPTIONS} value="two" onChange={() => {}} />);
  expect(screen.getByRole("group", { name: "View" })).toBeTruthy();
  const first = screen.getByRole("button", { name: "First view" });
  const second = screen.getByRole("button", { name: "Second view" });
  expect(first.getAttribute("aria-pressed")).toBe("false");
  expect(second.getAttribute("aria-pressed")).toBe("true");
  // The stylesheet draws this attribute as the width holder, kept out of the name by its CSS.
  expect(second.getAttribute("data-label")).toBe("Second view");
  expect(first.getAttribute("type")).toBe("button");
});

it("reports the value of the segment clicked", () => {
  const onChange = vi.fn();
  render(<SegmentedToggle label="View" options={OPTIONS} value="one" onChange={onChange} />);
  fireEvent.click(screen.getByRole("button", { name: "Second view" }));
  expect(onChange).toHaveBeenCalledExactlyOnceWith("two");
});
