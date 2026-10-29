import { act, cleanup, render, screen } from "@testing-library/react";
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

it("shows the running model, busy indicator and elapsed time ticking each second", () => {
  vi.useFakeTimers();
  vi.setSystemTime(65_000);

  render(<CodexStatusItem status={status} startedMs={1000} />);

  expect(screen.getByText(status.model)).toBeTruthy();
  expect(screen.getByRole("status").textContent).toContain("Codex is reviewing");
  expect(screen.getByText("Running · 1m4s")).toBeTruthy();
  act(() => vi.advanceTimersByTime(1000));
  expect(screen.getByText("Running · 1m5s")).toBeTruthy();
});

it("starts at zero when the start time is absent", () => {
  render(<CodexStatusItem status={status} />);

  expect(screen.getByText("Running · 0s")).toBeTruthy();
});

it("clamps elapsed time to zero when the start time is in the future", () => {
  vi.useFakeTimers();
  vi.setSystemTime(1000);

  render(<CodexStatusItem status={status} startedMs={2000} />);

  expect(screen.getByText("Running · 0s")).toBeTruthy();
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

it("removes the elapsed clock and busy indicator when the review finishes", () => {
  vi.useFakeTimers();
  vi.setSystemTime(2000);
  const { rerender } = render(<CodexStatusItem status={status} startedMs={1000} />);

  rerender(<CodexStatusItem status={{ ...status, state: "no_review" }} startedMs={1000} />);
  act(() => vi.advanceTimersByTime(1000));

  expect(screen.getByText("No review")).toBeTruthy();
  expect(screen.queryByText(/^Running/)).toBeNull();
  expect(screen.queryByRole("status")).toBeNull();
  expect(vi.getTimerCount()).toBe(0);
});

it("shows control characters in the log tail as visible markers", () => {
  const { container } = render(
    <CodexStatusItem status={{ ...status, state: "failed", log_tail: "rm ‮fdp.sh" }} />,
  );

  expect(container.querySelector("pre code")?.textContent).toBe("rm ⟨U+202E⟩fdp.sh");
});
