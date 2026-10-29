import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";

import { RoundDiagram } from "./round-diagram";

afterEach(cleanup);

function mount(rounds = 1) {
  return render(
    <RoundDiagram
      rounds={rounds}
      review={<span>Review picker</span>}
      secondReview={<span>Second review picker</span>}
      address={<span>Address picker</span>}
    />,
  );
}

it.each([
  [1, "Each round, 1 time"],
  [2, "Each round, 2 times"],
  [3, "Each round, 3 times"],
] as const)("labels the frame for %i rounds", (rounds, legend) => {
  mount(rounds);

  expect(screen.getByText(legend)).toBeTruthy();
});

it("drops the count from the legend when the rounds are not a number", () => {
  mount(Number.NaN);

  expect(screen.getByText("Each round")).toBeTruthy();
  expect(screen.queryByText(/NaN/)).toBeNull();
});

it("exposes one named frame group containing all three nodes", () => {
  mount();

  const frame = within(screen.getByRole("group", { name: "One pressure-test round" }));
  expect(frame.getByRole("region", { name: "Review" })).toBeTruthy();
  expect(frame.getByRole("region", { name: "Second review" })).toBeTruthy();
  expect(frame.getByRole("region", { name: "Address" })).toBeTruthy();
});

it.each([
  ["Review", "/pressure", "pressure"],
  ["Second review", "$pressure", "codex"],
  ["Address", "/address", "drafting"],
] as const)("renders the %s title, command chip and paused glyph", (title, command, kind) => {
  mount();

  const node = screen.getByRole("region", { name: title });
  expect(within(node).getByText(title)).toBeTruthy();
  expect(within(node).getByText(command).tagName).toBe("CODE");
  const glyph = node.querySelector(`[data-kind="${kind}"]`);
  expect(glyph?.getAttribute("data-paused")).toBe("true");
  expect(glyph?.getAttribute("aria-hidden")).toBe("true");
});

it("marks only the second review as background", () => {
  mount();

  const secondReview = screen.getByRole("region", { name: "Second review" });
  expect(within(secondReview).getByText("background")).toBeTruthy();
  expect(screen.getAllByText("background")).toHaveLength(1);
});

it.each([
  ["Review", "Review picker"],
  ["Second review", "Second review picker"],
  ["Address", "Address picker"],
] as const)("renders the slot inside %s", (title, slot) => {
  mount();

  const node = screen.getByRole("region", { name: title });
  expect(within(node).getByText(slot)).toBe(screen.getByText(slot));
});
