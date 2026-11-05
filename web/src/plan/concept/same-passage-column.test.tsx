import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import { S2, sent, src } from "./reading-column-fixtures";
import {
  active,
  button,
  group,
  installColumnTestHooks,
  mountProduct,
} from "./reading-column-harness";

installColumnTestHooks();

/** A and C open the same section; B, between them, opens another, so no fragment spans them. */
const sentences = () => [
  sent("a", "Alpha opens the intro.", [src(S2, "intro")]),
  sent("b", "Bravo opens the detail.", [src(S2, "detail")]),
  sent("c", "Charlie opens the intro.", [src(S2, "intro")]),
];

it("a passage shown twice is a reference the second time, which focuses the first showing", async () => {
  const { column } = mountProduct(sentences());

  fireEvent.click(button(column, /^Alpha opens the intro/));
  await group(column);
  fireEvent.click(button(column, /^Charlie opens the intro/));
  const reference = await within(column).findByRole("button", { name: "Same passage as above" });

  expect(within(column).getAllByText("Alpha text")).toHaveLength(1);
  expect(within(column).getAllByRole("button", { name: "Same passage as above" })).toHaveLength(1);
  expect(reference.closest("p.concept-muted")).not.toBeNull();
  const own = reference.closest(".concept-passage");
  expect(own?.querySelector(".concept-passage-chip")?.textContent).toMatch(/Intro/);

  fireEvent.click(reference);

  await waitFor(() => {
    const first = column.querySelector(".concept-passage[data-focus]");
    expect(first).not.toBe(own);
    expect(active()).toBe(first);
  });
});

it("the reference focuses the first showing even when that showing already holds the roving focus", async () => {
  const { column } = mountProduct(sentences());
  fireEvent.click(button(column, /^Alpha opens the intro/));
  await group(column);
  fireEvent.click(button(column, /^Charlie opens the intro/));
  const reference = await within(column).findByRole("button", { name: "Same passage as above" });
  const first = column.querySelector<HTMLElement>(".concept-passage[data-focus]")!;
  // A click inside the first showing's body focuses it; the reader then scrolls on and clicks
  // the reference, which takes the DOM focus while the roving key still names the first showing.
  act(() => first.focus());
  act(() => reference.focus());

  fireEvent.click(reference);

  await waitFor(() => expect(active()).toBe(first));
});
