import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, expect, it, vi } from "vitest";

import { KindControl } from "@/plan/new/kind-control";

afterEach(cleanup);

const radio = (name: string) => screen.getByRole("radio", { name }) as HTMLInputElement;

it("offers Feature and Standalone plan as radios named by their label, each described", () => {
  render(<KindControl small={false} onChange={() => {}} disabled={false} />);

  expect(radio("Feature").checked).toBe(true);
  expect(radio("Standalone plan").checked).toBe(false);
  const describedBy = (name: string) =>
    document.getElementById(radio(name).getAttribute("aria-describedby")!)?.textContent;
  expect(describedBy("Feature")).toBe("A vision, product sections, a metaplan and plans");
  expect(describedBy("Standalone plan")).toBe("One plan in doc/plans/");
});

it("reports the choice and moves the checked radio", () => {
  const onChange = vi.fn();
  function Harness() {
    const [small, setSmall] = useState(false);
    return (
      <KindControl
        small={small}
        onChange={(next) => {
          onChange(next);
          setSmall(next);
        }}
        disabled={false}
      />
    );
  }
  render(<Harness />);

  fireEvent.click(radio("Standalone plan"));

  expect(onChange).toHaveBeenCalledExactlyOnceWith(true);
  expect(radio("Standalone plan").checked).toBe(true);
  expect(radio("Feature").checked).toBe(false);
});

it("disables both radios together", () => {
  render(<KindControl small onChange={() => {}} disabled />);

  expect(radio("Feature").matches(":disabled")).toBe(true);
  expect(radio("Standalone plan").matches(":disabled")).toBe(true);
});
