import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { MergePanel } from "./merge-panel";

const destroyed = vi.hoisted(() => vi.fn());

vi.mock("./load", () => ({
  loadCodeMirror: () =>
    Promise.resolve({
      createMergeView(
        parent: HTMLElement,
        options: {
          left: string;
          right: string;
          onChange: (text: string) => void;
          onSave: () => void;
        },
      ) {
        const left = document.createElement("textarea");
        left.dataset.testid = "left";
        left.value = options.left;
        const right = document.createElement("textarea");
        right.dataset.testid = "right";
        right.value = options.right;
        right.addEventListener("input", () => options.onChange(right.value));
        right.addEventListener("keydown", (event) => {
          if (event.key === "s" && event.ctrlKey) {
            event.preventDefault();
            options.onSave();
          }
        });
        parent.append(left, right);
        return {
          destroy: destroyed,
          getText: () => right.value,
          setText: () => {},
          searchOpen: () => false,
        };
      },
    }),
}));

afterEach(cleanup);

describe("MergePanel", () => {
  it("shows both texts, reports edits on the right and destroys the view", async () => {
    const onChange = vi.fn();
    const { unmount } = render(
      <MergePanel left="now" right="mine" onChange={onChange} onSave={vi.fn()} />,
    );
    expect(((await screen.findByTestId("left")) as HTMLTextAreaElement).value).toBe("now");
    const right = (await screen.findByTestId("right")) as HTMLTextAreaElement;
    expect(right.value).toBe("mine");
    fireEvent.input(right, { target: { value: "merged" } });
    expect(onChange).toHaveBeenCalledWith("merged");
    unmount();
    expect(destroyed).toHaveBeenCalledTimes(1);
  });

  it("saves on Ctrl-S in the editable side without opening the browser dialog", async () => {
    const onSave = vi.fn();
    render(<MergePanel left="now" right="mine" onChange={vi.fn()} onSave={onSave} />);
    const right = await screen.findByTestId("right");
    expect(fireEvent.keyDown(right, { key: "s", ctrlKey: true })).toBe(false);
    expect(onSave).toHaveBeenCalledTimes(1);
  });
});
