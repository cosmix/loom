import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ValueControl } from "@/components/settings-control";

afterEach(() => {
  cleanup();
});

function renderNumberField(onCommit = vi.fn(), value = 24) {
  render(
    <ValueControl
      id="check-interval-hours"
      kind={{ type: "number" }}
      value={value}
      pending={false}
      invalid={false}
      label="check interval hours at user scope"
      onCommit={onCommit}
    />,
  );
  return {
    input: screen.getByRole("textbox", { name: "check interval hours at user scope" }),
    onCommit,
  };
}

describe("NumberField", () => {
  it("NumberField does not commit when the draft normalizes to the current value", () => {
    const { input: field, onCommit } = renderNumberField(vi.fn(), 42);

    fireEvent.change(field, { target: { value: "00042" } });
    fireEvent.keyDown(field, { key: "Enter" });

    expect(onCommit).not.toHaveBeenCalled();
  });

  it.each([
    ["900000", 900000],
    ["0", 0],
    ["4294967295", 4294967295],
    ["+5", 5],
    ["00042", 42],
    ["4294967296", "4294967296"],
    ["0x10", "0x10"],
    ["1e3", "1e3"],
    ["1.0000000000000001", "1.0000000000000001"],
    ["1.5", "1.5"],
    ["-1", "-1"],
    ["abc", "abc"],
  ])("NumberField commits %s as %s", (input, committed) => {
    const { input: field, onCommit } = renderNumberField();

    fireEvent.change(field, { target: { value: input } });
    fireEvent.keyDown(field, { key: "Enter" });

    expect(onCommit.mock.calls).toStrictEqual([[committed]]);
  });
});
