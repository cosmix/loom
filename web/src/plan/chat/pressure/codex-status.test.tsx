import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import type { CodexStatus } from "@/plan/api/schema-pressure";

import { CodexStatusItem } from "./codex-status";

const status: CodexStatus = {
  model: "gpt-6.1-terra",
  state: "running",
  report_path: null,
  log_tail: null,
};

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

it("shows only the model plus the screen-reader status while running", () => {
  vi.useFakeTimers();
  const { container } = render(<CodexStatusItem status={status} />);

  expect(container.querySelector("p")?.textContent).toBe(status.model);
  expect(screen.getByRole("status").textContent).toBe("Codex is reviewing");
  expect(screen.getByRole("status").className).toContain("sr-only");
  expect(container.textContent).not.toContain("·");
  expect(container.textContent).not.toMatch(/background|\$pressure|\d+m?\d*s\b/);
  expect(container.querySelector(".activity-tile, svg")).toBeNull();
  expect(container.querySelector(".font-mono")).toBeNull();
  expect(container.innerHTML).not.toMatch(/border|rounded|bg-background|px-3/);
  expect(vi.getTimerCount()).toBe(0);
});

it("links a written review to its encoded file-mode address", () => {
  render(
    <MemoryRouter>
      <CodexStatusItem
        status={{ ...status, state: "review_written", report_path: "doc/reviews/Codex review.md" }}
      />
    </MemoryRouter>,
  );

  expect(screen.getByText(status.model)).toBeTruthy();
  expect(screen.getByRole("link", { name: "Review written" }).getAttribute("href")).toBe(
    "/files/reviews/Codex%20review.md",
  );
  expect(screen.queryByRole("status")).toBeNull();
});

it("shows Review written without inventing a link when no report path is supplied", () => {
  render(<CodexStatusItem status={{ ...status, state: "review_written" }} />);

  expect(screen.getByText("Review written")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
});

it("shows the model and No review", () => {
  render(<CodexStatusItem status={{ ...status, state: "no_review" }} />);

  expect(screen.getByText(status.model)).toBeTruthy();
  expect(screen.getByText("No review")).toBeTruthy();
  expect(screen.queryByRole("status")).toBeNull();
});

it("renders a failed log tail literally as monospace text without creating HTML", () => {
  const tail = "Codex exited\n<img src=x onerror=alert(1)>\n& failure details";

  const { container } = render(
    <CodexStatusItem status={{ ...status, state: "failed", log_tail: tail }} />,
  );

  expect(screen.getByText(status.model)).toBeTruthy();
  expect(screen.getByText("Codex failed")).toBeTruthy();
  expect(container.querySelector("pre code")?.textContent).toBe(tail);
  expect(container.querySelector("pre")?.classList.contains("font-mono")).toBe(true);
  expect(container.querySelector("img")).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
});

it("swaps the running status for the outcome when the review finishes", () => {
  const { container, rerender } = render(<CodexStatusItem status={status} />);

  rerender(<CodexStatusItem status={{ ...status, state: "no_review" }} />);

  expect(container.querySelector("p")?.textContent).toBe(`${status.model} · No review`);
  expect(screen.queryByRole("status")).toBeNull();
});

it("shows control characters in the model name as visible markers", () => {
  const rlo = String.fromCodePoint(0x202e);

  render(<CodexStatusItem status={{ ...status, model: `gpt${rlo}6` }} />);

  expect(screen.getByText("gpt⟨U+202E⟩6")).toBeTruthy();
  expect(document.body.textContent).not.toContain(rlo);
});

it("shows control characters in the log tail as visible markers", () => {
  const { container } = render(
    <CodexStatusItem status={{ ...status, state: "failed", log_tail: "rm ‮fdp.sh" }} />,
  );

  expect(container.querySelector("pre code")?.textContent).toBe("rm ⟨U+202E⟩fdp.sh");
});

it("renders failed text under the model line without a card or tile", () => {
  const { container } = render(
    <CodexStatusItem status={{ ...status, state: "failed", log_tail: "boom" }} />,
  );

  expect(container.querySelector("p")?.textContent).toBe(`${status.model} · Codex failed`);
  expect(
    container.querySelector(".activity-tile, .activity-glyph, .plan-arrow-spinner"),
  ).toBeNull();
});
