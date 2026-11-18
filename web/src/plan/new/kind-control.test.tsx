import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";

import type { WorkKind } from "@/plan/new/kickoff-start";
import { KindControl } from "@/plan/new/kind-control";

afterEach(cleanup);

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;
const hintOf = (name: string) =>
  document.getElementById(radio(name).getAttribute("aria-describedby")!)!;

it("offers Feature, Standalone plan and Follow-up as radios named by their label, each described", () => {
  render(<KindControl kind="feature" onChange={() => {}} disabled={false} />);

  expect(screen.getAllByRole("radio")).toHaveLength(3);
  expect(radio("Feature").checked).toBe(true);
  expect(radio("Standalone plan").checked).toBe(false);
  expect(radio("Follow-up").checked).toBe(false);
  expect(hintOf("Feature").textContent).toBe("A vision, product sections, a metaplan and plans");
  expect(hintOf("Standalone plan").textContent).toBe("One plan in doc/plans/");
  expect(hintOf("Follow-up").textContent).toBe("New plans for a feature that is already built");
});

it("shows the chosen kind's hint beside the switch and keeps the others for their radios only", () => {
  render(<KindControl kind="plan" onChange={() => {}} disabled={false} />);

  expect(hintOf("Standalone plan").classList.contains("kickoff-kind__hint")).toBe(true);
  expect(hintOf("Feature").classList.contains("sr-only")).toBe(true);
  expect(hintOf("Follow-up").classList.contains("sr-only")).toBe(true);
});

it("lets what stands beside the switch replace the visible hint", () => {
  render(
    <KindControl kind="follow-up" onChange={() => {}} disabled={false} beside={<b>atlas</b>}>
      <p>floating</p>
    </KindControl>,
  );

  expect(hintOf("Follow-up").classList.contains("sr-only")).toBe(true);
  expect(screen.getByText("atlas")).toBeTruthy();
  const anchor = screen.getByText("floating").parentElement;
  expect(anchor?.classList.contains("kickoff-kind__anchor")).toBe(true);
  expect(anchor?.querySelector(".kickoff-kind__track")).toBeTruthy();
});

it("reports the chosen kind and moves the checked radio", () => {
  const onChange = vi.fn<(kind: WorkKind) => void>();
  function Harness() {
    const [kind, setKind] = useState<WorkKind>("feature");
    return (
      <KindControl
        kind={kind}
        onChange={(next) => {
          onChange(next);
          setKind(next);
        }}
        disabled={false}
      />
    );
  }
  render(<Harness />);

  fireEvent.click(radio("Follow-up"));
  expect(onChange).toHaveBeenCalledExactlyOnceWith("follow-up");
  expect(radio("Follow-up").checked).toBe(true);
  expect(radio("Feature").checked).toBe(false);

  fireEvent.click(radio("Standalone plan"));
  expect(onChange).toHaveBeenLastCalledWith("plan");
  expect(radio("Standalone plan").checked).toBe(true);
});

it("disables the three radios together", () => {
  render(<KindControl kind="plan" onChange={() => {}} disabled />);

  for (const name of ["Feature", "Standalone plan", "Follow-up"]) {
    expect(radio(name).matches(":disabled")).toBe(true);
  }
});
