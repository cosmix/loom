import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { ErrorLine } from "./error-line";
import { ImplementingPlans } from "./turn-footer-plans";

afterEach(cleanup);

it("draws the glyph aside and the words in the page's colour, and takes the caller's role", () => {
  render(<ErrorLine text="Boom" role="alert" />);
  const line = screen.getByRole("alert");
  expect(line.getAttribute("data-tone")).toBe("error");
  expect(line.textContent).toContain("Boom");
  expect(line.querySelector("[aria-hidden=true]")?.textContent).toBe("⚠");
});

it("is silent to assistive technology when the caller gives no role", () => {
  render(<ErrorLine text="Boom" />);
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByText(/Boom/)).toBeTruthy();
});

it("the implementing-plans failure offers Retry, which asks again", () => {
  const retry = vi.fn();
  render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <ImplementingPlans
          path="doc/product/demo/section_02.md"
          loaded={{ sections: null, error: "Plans did not load", retry }}
        />
      </MemoryRouter>
    </Provider>,
  );
  expect(screen.getByText(/Plans did not load/)).toBeTruthy();

  fireEvent.click(screen.getByRole("button", { name: "Retry" }));

  expect(retry).toHaveBeenCalledTimes(1);
});
