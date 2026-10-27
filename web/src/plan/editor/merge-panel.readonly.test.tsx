import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { MergePanel } from "./merge-panel";

const fake = vi.hoisted(() => ({
  setReadOnly: vi.fn(),
  destroy: vi.fn(),
  createMergeView: vi.fn(),
}));

vi.mock("./load", () => ({
  loadCodeMirror: () =>
    Promise.resolve({
      createMergeView: (parent: HTMLElement, options: unknown) => {
        fake.createMergeView(options);
        const marker = document.createElement("div");
        marker.dataset.testid = "merge";
        parent.append(marker);
        return {
          destroy: fake.destroy,
          getText: () => "",
          setText: () => {},
          searchOpen: () => false,
          setReadOnly: fake.setReadOnly,
        };
      },
    }),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const props = { left: "now", right: "mine", onChange: vi.fn(), onSave: vi.fn() };

describe("MergePanel read-only", () => {
  it("builds the view read-only when authoring is lost before it opens", async () => {
    render(<MergePanel {...props} readOnly />);
    await screen.findByTestId("merge");
    expect(fake.createMergeView).toHaveBeenCalledWith(
      expect.objectContaining({ left: "now", right: "mine", readOnly: true }),
    );
  });

  it("follows read-only changes without rebuilding the view", async () => {
    const { rerender } = render(<MergePanel {...props} />);
    await screen.findByTestId("merge");
    expect(fake.createMergeView).toHaveBeenCalledWith(expect.objectContaining({ readOnly: false }));

    rerender(<MergePanel {...props} readOnly />);
    expect(fake.setReadOnly).toHaveBeenLastCalledWith(true);
    rerender(<MergePanel {...props} />);
    expect(fake.setReadOnly).toHaveBeenLastCalledWith(false);
    expect(fake.createMergeView).toHaveBeenCalledTimes(1);
    expect(fake.destroy).not.toHaveBeenCalled();
  });
});
