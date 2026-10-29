import { readFileSync } from "node:fs";
import { join } from "node:path";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PinnedScene, type SceneNote } from "./pinned-scene";

const css = readFileSync(join(import.meta.dirname, "pinned-scene.css"), "utf8");
const notes: readonly SceneNote[] = [
  { target: ".identity", text: "Name the plan here." },
  { target: ".action", text: "Start the next step." },
  { target: ".missing", text: "An unavailable control has no pin." },
];
const observers: ResizeObserverStub[] = [];
let targetOffset = 0;
let sampleHeight = 100;

class ResizeObserverStub {
  observe = vi.fn();
  disconnect = vi.fn();
  callback: () => void;

  constructor(callback: () => void) {
    this.callback = callback;
    observers.push(this);
  }
}

function rect(x: number, y: number, width: number, height: number): DOMRect {
  return {
    x,
    y,
    left: x,
    top: y,
    width,
    height,
    right: x + width,
    bottom: y + height,
    toJSON: () => ({}),
  };
}

function measuredRect(this: HTMLElement): DOMRect {
  if (this.classList.contains("planner-help-scene__canvas")) return rect(100, 50, 600, 164);
  const stage = this.closest<HTMLElement>(".planner-help-scene__stage");
  const scale = Number(stage?.style.transform.match(/scale\(([^)]+)\)/)?.[1] ?? 1);
  if (this === stage) return rect(132, 82, 536, sampleHeight * scale);
  const x = this.dataset.x;
  if (x === undefined) return rect(0, 0, 0, 0);
  return rect(
    132 + (Number(x) + targetOffset) * scale,
    82 + Number(this.dataset.y) * scale,
    Number(this.dataset.width) * scale,
    20 * scale,
  );
}

function Example({
  sceneNotes = notes,
  scale = 1,
}: {
  sceneNotes?: readonly SceneNote[];
  scale?: number;
}) {
  return (
    <PinnedScene number={2} title="Plan controls" notes={sceneNotes} scale={scale}>
      <button className="identity control" data-x="48" data-y="20" data-width="80">
        Sample name
      </button>
      <button className="action control" data-x="268" data-y="58" data-width="40">
        Sample action
      </button>
    </PinnedScene>
  );
}

beforeEach(() => {
  targetOffset = 0;
  sampleHeight = 100;
  observers.length = 0;
  vi.useFakeTimers();
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(measuredRect);
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("PinnedScene", () => {
  it("renders its scene number, title, hint and accessible ordered notes", () => {
    const { container } = render(<Example />);
    expect(screen.getByText("Scene 2")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Plan controls" })).toBeTruthy();
    expect(screen.getByText("Hover a note to find it")).toBeTruthy();
    expect(screen.getByRole("list").tagName).toBe("OL");
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    for (const note of notes) expect(screen.getByText(note.text)).toBeTruthy();
    for (const note of screen.getAllByRole("listitem")) expect(note.tabIndex).toBe(0);
    expect(container.querySelector(".planner-help-scene__stage")?.hasAttribute("inert")).toBe(true);
  });

  it("places one decorative pin per matched target and skips missing selectors", () => {
    const { container } = render(<Example />);
    const pins = container.querySelectorAll<HTMLElement>(".planner-help-scene__pin");
    expect([...pins].map((pin) => pin.textContent)).toEqual(["1", "2"]);
    expect(pins[0].style.left).toBe("109px");
    expect(pins[0].style.top).toBe("24px");
    expect(pins[0].closest('[aria-hidden="true"]')).toBeTruthy();
    expect(screen.getByText(notes[2].text)).toBeTruthy();
  });

  it("activates a pin and rings its measured target while a note is hovered", () => {
    const { container } = render(<Example />);
    const note = screen.getAllByRole("listitem")[0];
    fireEvent.mouseEnter(note);
    expect(
      container.querySelector('.planner-help-scene__pin[data-active="true"]')?.textContent,
    ).toBe("1");
    const ring = container.querySelector<HTMLElement>(".planner-help-scene__ring")!;
    expect([ring.style.left, ring.style.top, ring.style.width, ring.style.height]).toEqual([
      "80px",
      "52px",
      "80px",
      "20px",
    ]);
    fireEvent.mouseLeave(note);
    expect(container.querySelector(".planner-help-scene__ring")).toBeNull();
    expect(container.querySelector(".planner-help-scene__pin[data-active]")).toBeNull();
  });

  it("supports keyboard focus and preserves it after a hover ends", () => {
    const { container } = render(<Example />);
    const items = screen.getAllByRole("listitem");
    fireEvent.focus(items[0]);
    expect(container.querySelector(".planner-help-scene__pin[data-active]")?.textContent).toBe("1");
    fireEvent.mouseEnter(items[1]);
    expect(container.querySelector(".planner-help-scene__pin[data-active]")?.textContent).toBe("2");
    fireEvent.mouseLeave(items[1]);
    expect(container.querySelector(".planner-help-scene__pin[data-active]")?.textContent).toBe("1");
    fireEvent.blur(items[0]);
    expect(container.querySelector(".planner-help-scene__ring")).toBeNull();
  });

  it.each<{ target: SceneNote["target"] }>([
    { target: ".control" },
    { target: [".identity", ".action", ".missing"] },
  ])("anchors a single pin to the union of every match for $target", ({ target }) => {
    const { container } = render(
      <Example sceneNotes={[{ target, text: "The whole control group." }]} />,
    );
    expect(container.querySelectorAll(".planner-help-scene__pin")).toHaveLength(1);
    fireEvent.focus(screen.getByRole("listitem"));
    const ring = container.querySelector<HTMLElement>(".planner-help-scene__ring")!;
    expect([ring.style.left, ring.style.top, ring.style.width, ring.style.height]).toEqual([
      "80px",
      "52px",
      "260px",
      "58px",
    ]);
  });

  it("uses transformed target rectangles and reserves only the scaled stage height", () => {
    const { container } = render(<Example scale={0.74} />);
    const stage = container.querySelector<HTMLElement>(".planner-help-scene__stage")!;
    expect(stage.style.transform).toBe("scale(0.74)");
    expect(Number.parseFloat(stage.style.width)).toBeCloseTo(100 / 0.74);
    expect(container.querySelector<HTMLElement>(".planner-help-scene__frame")?.style.height).toBe(
      "74px",
    );
    fireEvent.focus(screen.getAllByRole("listitem")[0]);
    const ring = container.querySelector<HTMLElement>(".planner-help-scene__ring")!;
    expect(Number.parseFloat(ring.style.left)).toBeCloseTo(32 + 48 * 0.74);
    expect(Number.parseFloat(ring.style.width)).toBeCloseTo(80 * 0.74);
    expect(container.querySelector<HTMLElement>(".planner-help-scene__canvas")?.style.padding).toBe(
      "32px",
    );
  });

  it("remeasures observed layouts and cancels observers and timers on unmount", () => {
    const { container, unmount } = render(<Example />);
    expect(observers[0].observe).toHaveBeenCalledTimes(4);
    targetOffset = 20;
    sampleHeight = 150;
    act(() => observers[0].callback());
    expect(container.querySelector<HTMLElement>(".planner-help-scene__pin")?.style.left).toBe(
      "129px",
    );
    expect(container.querySelector<HTMLElement>(".planner-help-scene__frame")?.style.height).toBe(
      "150px",
    );
    unmount();
    expect(observers[0].disconnect).toHaveBeenCalledOnce();
    expect(vi.getTimerCount()).toBe(0);
  });

  it.each([80, 300, 800])("remeasures delayed layout after %i ms", (delay) => {
    const { container } = render(<Example />);
    targetOffset = 10;
    act(() => vi.advanceTimersByTime(delay));
    expect(container.querySelector<HTMLElement>(".planner-help-scene__pin")?.style.left).toBe(
      "119px",
    );
  });

  it("discovers targets that appear after mount", () => {
    const { container } = render(<Example />);
    const button = container.querySelector(".action")!;
    button.classList.add("missing");
    act(() => vi.advanceTimersByTime(80));
    expect(container.querySelectorAll(".planner-help-scene__pin")).toHaveLength(3);
    fireEvent.focus(screen.getAllByRole("listitem")[2]);
    expect(container.querySelector(".planner-help-scene__ring")).toBeTruthy();
  });

  it("keeps an unmatched note focusable without drawing a ring", () => {
    const { container } = render(<Example />);
    fireEvent.focus(screen.getAllByRole("listitem")[2]);
    expect(container.querySelector(".planner-help-scene__ring")).toBeNull();
    expect(container.querySelector(".planner-help-scene__pin[data-active]")).toBeNull();
  });

  it("disables pin transitions and halo pulses for reduced motion", () => {
    const reducedMotion = css.slice(css.indexOf("@media (prefers-reduced-motion: reduce)"));
    expect(reducedMotion).toContain("@media (prefers-reduced-motion: reduce)");
    expect(reducedMotion).toMatch(/\.planner-help-scene__pin\s*\{\s*transition:\s*none;/);
    expect(reducedMotion).toMatch(
      /\.planner-help-scene__pin\[data-active\]::before\s*\{\s*animation:\s*none;/,
    );
  });
});
