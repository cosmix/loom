import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { StageDescription } from "@/components/stage-detail";

describe("StageDescription", () => {
  it("shows the plan-authored description, preserving line breaks", () => {
    const { container } = render(
      <StageDescription description={"Wires the button.\nHandles the click."} />,
    );

    const paragraph = container.querySelector("p");
    expect(paragraph?.textContent).toBe("Wires the button.\nHandles the click.");
    expect(paragraph?.className).toContain("whitespace-pre-line");
  });

  it("renders no block when the stage has no description", () => {
    const { container } = render(<StageDescription description={null} />);

    expect(container.innerHTML).toBe("");
  });
});
