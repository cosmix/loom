import { cleanup, render, screen } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, expect, it } from "vitest";

import { RouteError } from "@/routes/error";

afterEach(cleanup);

it.each([
  ["planner", "404 Not Found in the planner", "Back to Home"],
  ["dashboard", "404 Not Found", "back to the overview"],
  [undefined, "404 Not Found", "back to the overview"],
] as const)("renders an unknown route for %s", (appName, title, linkText) => {
  const router = createMemoryRouter(
    [{ path: "/", element: <p>Home</p>, errorElement: <RouteError appName={appName} /> }],
    { initialEntries: ["/unknown-route"] },
  );
  render(<RouterProvider router={router} />);

  expect(screen.getByText(title)).toBeTruthy();
  expect(screen.getByRole("link", { name: linkText }).getAttribute("href")).toBe("/");
});
