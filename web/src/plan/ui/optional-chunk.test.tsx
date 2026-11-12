import { cleanup, render, screen } from "@testing-library/react";
import { lazy } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { OptionalChunk } from "./optional-chunk";

afterEach(cleanup);

it("shows the component once its chunk arrives", async () => {
  const Dialog = lazy(() => Promise.resolve({ default: () => <p>Dialog body</p> }));
  render(
    <OptionalChunk>
      <Dialog />
    </OptionalChunk>,
  );
  expect(await screen.findByText("Dialog body")).toBeTruthy();
});

it("draws nothing, and leaves the rest of the screen, when the chunk fails to load", async () => {
  const log = vi.spyOn(console, "error").mockImplementation(() => {});
  const Dialog = lazy(() =>
    Promise.reject(new Error("Failed to fetch dynamically imported module")),
  );
  render(
    <div>
      <p>The planner</p>
      <OptionalChunk>
        <Dialog />
      </OptionalChunk>
    </div>,
  );
  await vi.waitFor(() => expect(log).toHaveBeenCalled());
  expect(screen.getByText("The planner")).toBeTruthy();
  expect(screen.queryByText("Something went wrong")).toBeNull();
  expect(screen.queryByRole("button", { name: "Reload" })).toBeNull();
  log.mockRestore();
});
