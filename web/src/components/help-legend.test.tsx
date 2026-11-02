import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { LegendTopic } from "@/components/help-legend";
import { HELP_TOPICS } from "@/components/help-topics";

afterEach(cleanup);

/// An open tool call earns `busy` only while its foreground command keeps
/// progressing inside a bounded hold (`foreground_command_active`); past that it
/// reads `stale` with the call still open, so neither word may promise otherwise.
const OLD_PROMISES = [
  /no tool call open/i,
  /a tool call is still open/i,
  /open tool call shows as busy/i,
];

describe("activity vocabulary", () => {
  it("describes busy as a bounded hold and allows a stale open call in the legend", () => {
    const { container } = render(<LegendTopic />);
    for (const promise of OLD_PROMISES) {
      expect(container.textContent).not.toMatch(promise);
    }
    expect(screen.getByText(/still making progress, within a bounded hold/)).toBeTruthy();
    expect(screen.getByText(/a long tool call can still read stale/)).toBeTruthy();
  });

  it("says the same in the help guide's tips", () => {
    const tips = HELP_TOPICS.flatMap((topic) => topic.tips ?? []).join("\n");
    for (const promise of OLD_PROMISES) {
      expect(tips).not.toMatch(promise);
    }
    expect(tips).toMatch(/still making progress, within a bounded hold/);
    expect(tips).toMatch(/a long tool call can still read stale/);
  });
});
