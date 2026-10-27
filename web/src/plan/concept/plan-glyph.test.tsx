import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import { PlanGlyph } from "./plan-glyph";

afterEach(cleanup);

it("a plan link encodes the slug the way the page header does", async () => {
  const plans = [{ slug: "a b#c", title: "Odd slug", state: "running" }];
  render(
    <MemoryRouter>
      <PlanGlyph plans={plans} sections={[]} />
    </MemoryRouter>,
  );
  fireEvent.click(screen.getByRole("button", { name: "Plans implementing this section" }));
  const link = await screen.findByRole("link", { name: "Odd slug" });
  expect(link.getAttribute("href")).toBe("/engineering/a%20b%23c");
});
