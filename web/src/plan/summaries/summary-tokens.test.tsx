import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { SummaryTokens } from "./summary-tokens";

afterEach(cleanup);

describe("SummaryTokens", () => {
  it("renders a token count with thousands separators", () => {
    render(<SummaryTokens tokens={12000} />);
    expect(screen.getByText("12,000 tokens")).toBeTruthy();
  });

  it("renders an unknown token count", () => {
    render(<SummaryTokens tokens={null} />);
    expect(screen.getByText("tokens unknown")).toBeTruthy();
  });
});
