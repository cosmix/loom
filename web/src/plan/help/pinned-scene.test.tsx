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
// How much an ancestor of the canvas (the dialog zooming in) scales everything on screen.
let ancestorScale = 1;

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
  const a = ancestorScale;
  if (this.classList.contains("planner-help-scene__canvas")) return rect(100, 50, 600 * a, 164 * a);
  const stage = this.closest<HTMLElement>(".planner-help-scene__stage");
  const scale = Number(stage?.style.transform.match(/scale\(([^)]+)\)/)?.[1] ?? 1);
  if (this === stage) return rect(100 + 32 * a, 50 + 32 * a, 536 * a, sampleHeight * scale * a);
  const x = this.dataset.x;
  if (x === undefined) return rect(0, 0, 0, 0);
  return rect(
    100 + (32 + (Number(x) + targetOffset) * scale) * a,
    50 + (32 + Number(this.dataset.y) * scale) * a,
    Number(this.dataset.width) * scale * a,
    20 * scale * a,
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
  ancestorScale = 1;
  observers.length = 0;
  vi.useFakeTimers();
  vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(measuredRect);
  // The canvas is 600px wide in its own layout; ancestors may scale it on screen.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.classList.contains("planner-help-scene__canvas") ? 600 : 1;
  });
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

  it("divides out an ancestor's scale so pins stay in the canvas's own pixels", () => {
    ancestorScale = 0.5;
    const { container } = render(<Example />);

    expect(container.querySelector<HTMLElement>(".planner-help-scene__pin")?.style.left).toBe(
      "109px",
    );
    expect(container.querySelector<HTMLElement>(".planner-help-scene__frame")?.style.height).toBe(
      "100px",
    );
    fireEvent.focus(screen.getAllByRole("listitem")[0]);
    const ring = container.querySelector<HTMLElement>(".planner-help-scene__ring")!;
    expect([ring.style.left, ring.style.top, ring.style.width, ring.style.height]).toEqual([
      "80px",
      "52px",
      "80px",
      "20px",
    ]);
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

  it("lets a panel scene's stage hug its content and centre in the canvas", () => {
    const plain = render(<Example />);
    const stage = plain.container.querySelector<HTMLElement>(".planner-help-scene__stage")!;
    expect(stage.hasAttribute("data-panel")).toBe(false);
    expect(stage.style.width).toBe("100%");
    plain.unmount();

    const { container } = render(
      <PinnedScene number={1} title="Panel" notes={notes} panel>
        <button className="identity" data-x="48" data-y="20" data-width="80" />
      </PinnedScene>,
    );
    const panel = container.querySelector<HTMLElement>(".planner-help-scene__stage")!;
    expect(panel.hasAttribute("data-panel")).toBe(true);
    expect(panel.style.width).toBe("");
    expect(css).toMatch(
      /\[data-panel\]\s*\{\s*width:\s*fit-content;\s*max-width:\s*100%;\s*margin-inline:\s*auto;/,
    );
  });

  it("keeps a pin off the text the stage lays out beside its target", () => {
    // jsdom has no layout for text, so give every text run one line box just above the target,
    // where its pin would otherwise go (the target is at x 80, y 52 in the canvas).
    Object.defineProperty(Range.prototype, "getClientRects", {
      configurable: true,
      value: () => [rect(100 + 32 + 40, 50 + 32 + 20 - 18, 120, 14)],
    });
    try {
      const { container } = render(
        <PinnedScene number={1} title="Text" notes={[notes[0]!]}>
          <p>A line of text over the control.</p>
          <button className="identity control" data-x="48" data-y="20" data-width="80">
            Sample name
          </button>
        </PinnedScene>,
      );
      const pin = container.querySelector<HTMLElement>(".planner-help-scene__pin")!;

      // Above would be x 109, y 24; it is taken by the text, so the pin hangs to the left.
      expect([pin.style.left, pin.style.top]).toEqual(["52px", "51px"]);
    } finally {
      Reflect.deleteProperty(Range.prototype, "getClientRects");
    }
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
