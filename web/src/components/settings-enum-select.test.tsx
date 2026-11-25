import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ValueControl } from "@/components/settings-control";

afterEach(() => {
  cleanup();
});

function renderEnumSelect() {
  render(
    <ValueControl
      id="summary-backend"
      kind={{ type: "enum", variants: ["claude", "codex"] }}
      value="screen"
      pending={false}
      invalid={false}
      label="summary_backend at user scope"
      onCommit={vi.fn()}
    />,
  );
  return { select: screen.getByRole("combobox", { name: "summary_backend at user scope" }) };
}

describe("EnumSelect", () => {
  it("EnumSelect keeps an unlisted current value as an option", () => {
    const { select } = renderEnumSelect();

    expect(select).toHaveProperty("value", "screen");
    const options = Array.from(select.querySelectorAll("option")).map((option) => option.value);
    expect(options).toStrictEqual(["screen", "claude", "codex"]);
  });
});
