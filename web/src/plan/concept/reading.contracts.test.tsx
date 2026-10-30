import { act, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { active, group, installColumnTestHooks, press } from "./reading-column-harness";
import { chipLinks, openableOverview, showKey } from "./reading-contracts-kit";

installColumnTestHooks();

describe("reading repair", () => {
  it("enter opens the focused sentence", async () => {
    const { column } = openableOverview();
    act(() => column.focus());
    await waitFor(() => expect(active().getAttribute("data-sentence-id")).toBe("o"));

    // `j` moves only between openable sentences and caps: the plain line after "o" is skipped.
    press("j");
    press("Enter");

    expect(await group(column)).toBeTruthy();
  });

  it("document key shows four chips and a more chip", () => {
    const hrefs = showKey(12);

    expect(chipLinks(hrefs)).toHaveLength(4);
    expect(screen.getByRole("button", { name: /^…\s?8 more$/ })).toBeTruthy();
  });
});
