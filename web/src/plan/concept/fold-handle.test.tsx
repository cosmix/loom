import { act, cleanup, render, screen } from "@testing-library/react";
import { useRef } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { FoldHandle } from "./fold-handle";

type Watcher = { fire: () => void; observed: Element[]; disconnected: boolean };

let watchers: Watcher[] = [];
const runTop = 10;
let runBottom = 30;

class RecordingObserver {
  private readonly watcher: Watcher;
  constructor(callback: ResizeObserverCallback) {
    this.watcher = { fire: () => callback([], this), observed: [], disconnected: false };
    watchers.push(this.watcher);
  }
  observe(element: Element) {
    this.watcher.observed.push(element);
  }
  unobserve() {}
  disconnect() {
    this.watcher.disconnected = true;
  }
}

/** The observer the mounted handle currently holds; earlier ones were disconnected. */
const live = () => watchers.filter((watcher) => !watcher.disconnected);

function Harness() {
  const ref = useRef<HTMLSpanElement>(null);
  return (
    <p className="concept-paragraph">
      <span ref={ref} id="run-1">
        opened text
      </span>
      <FoldHandle target={ref} depth={0} onFold={() => {}} />
    </p>
  );
}

beforeEach(() => {
  watchers = [];
  runBottom = 30;
  vi.stubGlobal("ResizeObserver", RecordingObserver);
  vi.spyOn(Element.prototype, "getClientRects").mockImplementation(
    () => [{ top: runTop, bottom: runBottom }] as unknown as DOMRectList,
  );
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockImplementation(
    () => ({ top: 0 }) as DOMRect,
  );
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("FoldHandle", () => {
  it("spans the run's lines inside its paragraph", () => {
    render(<Harness />);
    const handle = screen.getByRole("button", { name: "Fold" });
    expect(handle.style.top).toBe("10px");
    expect(handle.style.height).toBe("20px");
  });

  it("observes the paragraph as well as the inline run", () => {
    const { container } = render(<Harness />);
    const observer = live();
    expect(observer).toHaveLength(1);
    expect(observer[0]!.observed).toContain(container.querySelector("#run-1"));
    expect(observer[0]!.observed).toContain(container.querySelector(".concept-paragraph"));
  });

  it("re-measures when the paragraph resizes, so a loaded nested passage lengthens it", () => {
    render(<Harness />);
    runBottom = 70;
    act(() => live()[0]!.fire());
    expect(screen.getByRole("button", { name: "Fold" }).style.height).toBe("60px");
  });

  it("is described by the run it folds", () => {
    render(<Harness />);
    expect(screen.getByRole("button", { name: "Fold" }).getAttribute("aria-describedby")).toBe(
      "run-1",
    );
  });
});
