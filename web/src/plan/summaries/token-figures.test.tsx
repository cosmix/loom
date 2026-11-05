import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { stubMotion } from "./motion-stub";
import { todaySpan, TokenBar, TokenCards, totalTokens } from "./token-figures";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

const tickAt = (container: HTMLElement) =>
  container.querySelector<HTMLElement>(".sp-tick")?.style.left ?? null;
const widths = (container: HTMLElement) =>
  Array.from(container.querySelectorAll<HTMLElement>(".sp-segment"), (s) => s.style.width);

describe("TokenBar scale", () => {
  it("fills the track when today runs above the daily average and marks the average inside it", () => {
    const today = { fresh_input: 700, output: 300 };
    const weekTotal = 3500;
    const span = todaySpan(1000, weekTotal);
    expect(span).toBe(1000);

    const { container } = render(<TokenBar tokens={today} span={span} tick={weekTotal / 7} />);

    expect(widths(container)).toEqual(["70%", "30%"]);
    expect(tickAt(container)).toBe("50%");
  });

  it("scales today's track to the daily average when today runs under it", () => {
    const today = { fresh_input: 250 };
    const weekTotal = 7000;
    const span = todaySpan(250, weekTotal);
    expect(span).toBe(1000);

    const { container } = render(<TokenBar tokens={today} span={span} tick={weekTotal / 7} />);

    expect(widths(container)).toEqual(["25%"]);
    expect(tickAt(container)).toBe("100%");
  });

  it("announces each segment as its label and figure, skipping zero and absent components", () => {
    const { container } = render(
      <TokenBar tokens={{ fresh_input: 1500, cache_write: 0, output: 500 }} span={2000} />,
    );

    expect(
      Array.from(container.querySelectorAll('[role="img"]'), (s) => s.getAttribute("aria-label")),
    ).toEqual(["fresh 1,500", "output 500"]);
    expect(tickAt(container)).toBeNull();
  });

  it("draws nothing on an empty track and no tick for an empty week", () => {
    const empty = render(<TokenBar tokens={{}} span={0} />);
    expect(widths(empty.container)).toEqual([]);
    expect(tickAt(empty.container)).toBeNull();
    cleanup();

    const { container } = render(<TokenBar tokens={{ fresh_input: 5 }} span={5} tick={0} />);
    expect(widths(container)).toEqual(["100%"]);
    expect(tickAt(container)).toBeNull();
  });
});

it("totals only the recorded components, telling an absent vector from a recorded zero", () => {
  expect(totalTokens({})).toBeNull();
  expect(totalTokens({ fresh_input: 0 })).toBe(0);
  expect(totalTokens({ fresh_input: 1, cache_read: 2, cache_write: 3, output: 4 })).toBe(10);
});

const TODAY = { fresh_input: 700, output: 300 };
const WEEK = { fresh_input: 3500, cache_read: 3500 };
const total = (name: string) =>
  screen.getByRole("group", { name }).querySelector(".sp-total-figure")!.textContent;

describe("the count-up", () => {
  it("runs from 0 to the total over 400ms of frames when the agent allows motion", () => {
    stubMotion((query) => query.includes("no-preference"));
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
    render(<TokenCards today={TODAY} week={WEEK} />);

    expect(total("Today")).toBe("0");
    act(() => {
      vi.advanceTimersByTime(200);
    });
    const midway = Number(total("Today"));
    expect(midway).toBeGreaterThan(0);
    expect(midway).toBeLessThan(1000);
    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(total("Today")).toBe("1,000");
    expect(total("7 days")).toBe("7,000");
  });

  it("renders the final figure at once when the agent reports both answers or none", () => {
    stubMotion(() => true);
    render(<TokenCards today={TODAY} week={WEEK} />);
    expect(total("Today")).toBe("1,000");
    cleanup();

    render(<TokenCards today={TODAY} week={WEEK} />);
    expect(total("7 days")).toBe("7,000");
  });

  it("cancels its frame on unmount", () => {
    stubMotion((query) => query.includes("no-preference"));
    vi.useFakeTimers({ toFake: ["requestAnimationFrame", "cancelAnimationFrame", "performance"] });
    const cancel = vi.spyOn(globalThis, "cancelAnimationFrame");
    const { unmount } = render(<TokenCards today={TODAY} week={WEEK} />);

    unmount();

    expect(cancel).toHaveBeenCalled();
  });
});
