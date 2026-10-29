import { cleanup, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import { PlanHeader } from "@/plan/header/header";

vi.mock("@/plan/header-extras", () => ({
  HeaderExtras: () => <button type="button">Chat</button>,
}));
vi.mock("@/plan/header/name-prompt", () => ({ useResendStoredName: () => {} }));
vi.mock("@/plan/header/overflow", () => ({ Overflow: () => <button type="button">More</button> }));
vi.mock("@/plan/header/presence", () => ({ Presence: () => null }));
vi.mock("@/plan/header/share", () => ({ Share: () => null }));
vi.mock("@/plan/header/step-tracker", () => ({ StepTracker: () => null }));

afterEach(cleanup);

function show() {
  return render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <PlanHeader />
      </MemoryRouter>
    </Provider>,
  );
}

it("keeps the right-hand controls at full size and lets everything else give way", () => {
  show();
  const controls = screen.getByRole("button", { name: "Chat" }).parentElement!;
  expect(controls.className).toContain("shrink-0");
  expect(controls.className).toContain("ml-auto");
  expect(screen.getByRole("navigation", { name: "Planning tabs" }).className).toContain("min-w-0");
});

it("takes the wordmark out of the layout below sm without hiding it from screen readers", () => {
  show();
  const wordmark = screen.getByText("loom plan");
  expect(wordmark.className).toContain("max-sm:sr-only");
  expect(wordmark.className).not.toContain("hidden");
});
