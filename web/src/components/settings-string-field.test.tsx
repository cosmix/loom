import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ValueControl } from "@/components/settings-control";

afterEach(() => {
  cleanup();
});

function renderStringField(onCommit = vi.fn()) {
  render(
    <ValueControl
      id="notes-title"
      kind={{ type: "string" }}
      value="My Loom notes"
      pending={false}
      invalid={false}
      label="title at user scope"
      onCommit={onCommit}
    />,
  );
  return { input: screen.getByRole("textbox", { name: "title at user scope" }), onCommit };
}

describe("StringField", () => {
  it("StringField renders its value", () => {
    const { input } = renderStringField();

    expect(input).toHaveProperty("value", "My Loom notes");
  });

  it("StringField commits the trimmed text", () => {
    const { input, onCommit } = renderStringField();

    fireEvent.change(input, { target: { value: "  my project " } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onCommit).toHaveBeenCalledWith("my project");
  });

  it("StringField reverts its draft on Escape", () => {
    const { input, onCommit } = renderStringField();

    fireEvent.change(input, { target: { value: "unsaved" } });
    fireEvent.keyDown(input, { key: "Escape" });

    expect(input).toHaveProperty("value", "My Loom notes");
    expect(onCommit).not.toHaveBeenCalled();
  });
});
