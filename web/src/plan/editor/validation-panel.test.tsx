import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import invalidFixture from "@/plan/api/fixtures/files-save-invalid.json";
import { validationViewSchema, type ValidationView } from "@/plan/api/schema-files";

import { errorLines, hasFindings, ValidationPanel } from "./validation-panel";

afterEach(cleanup);

const validation: ValidationView = validationViewSchema.parse({
  ...invalidFixture.validation,
  errors: [
    { message: "stage x has no acceptance", stage_id: "x", line: 12, column: null },
    { message: "no metadata block", stage_id: null, line: null, column: null },
  ],
  warnings: { structural: ["loose heading"], knowledge: [], sandbox: ["wide write"], baseline: [] },
  notes: ["3 stages"],
});

describe("ValidationPanel", () => {
  it("lists errors, warnings by bucket and notes", () => {
    render(<ValidationPanel validation={validation} onJump={() => {}} />);
    expect(screen.getByText("stage x has no acceptance")).toBeTruthy();
    expect(screen.getByText("no metadata block")).toBeTruthy();
    expect(screen.getByRole("region", { name: "Warnings: Structure" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Warnings: Sandbox" })).toBeTruthy();
    expect(screen.queryByRole("region", { name: "Warnings: Knowledge" })).toBeNull();
    expect(screen.getByText("3 stages")).toBeTruthy();
  });

  it("jumps to the line of an error that has one", () => {
    const onJump = vi.fn();
    render(<ValidationPanel validation={validation} onJump={onJump} />);
    expect(screen.getAllByRole("button", { name: /line/ })).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: "line 12" }));
    expect(onJump).toHaveBeenCalledWith(12);
  });

  it("offers Save with errors only when asked", () => {
    const onSave = vi.fn();
    const { rerender } = render(<ValidationPanel validation={validation} onJump={() => {}} />);
    expect(screen.queryByRole("button", { name: /save with errors/i })).toBeNull();
    rerender(
      <ValidationPanel validation={validation} onJump={() => {}} onSaveWithErrors={onSave} />,
    );
    fireEvent.click(screen.getByRole("button", { name: /save with errors/i }));
    expect(onSave).toHaveBeenCalledTimes(1);
  });

  it("reports gutter lines and whether there is anything to show", () => {
    expect(errorLines(validation)).toEqual([12]);
    expect(errorLines(null)).toEqual([]);
    expect(hasFindings(validation)).toBe(true);
    expect(hasFindings(null)).toBe(false);
    const empty = { ...validation, errors: [], notes: [] };
    expect(
      hasFindings({
        ...empty,
        warnings: { structural: [], knowledge: [], sandbox: [], baseline: [] },
      }),
    ).toBe(false);
  });
});
