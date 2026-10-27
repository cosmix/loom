import { describe, expect, it } from "vitest";
import { parseAddress } from "@/plan/files/address";
import { openDiff } from "./open-diff";

describe("openDiff", () => {
  it("builds addresses for each diff kind", () => {
    expect(openDiff("head", "doc/plans/a space.md")).toBe("/files/plans/a%20space.md?diff=head");
    expect(openDiff("open", "plans/x.md")).toBe("/files/plans/x.md?diff=open");
    expect(openDiff("turn", "doc/plans/x.md", "session/t2")).toBe(
      "/files/plans/x.md?diff=turn&turn=session%2Ft2",
    );
    expect(openDiff("turn", "doc/plans/x.md")).toBe("/files/plans/x.md?diff=turn");
  });

  it("round trips a full turn ID and a path with reserved characters", () => {
    const path = "doc/plans/a space/100%.md";
    const turn = "session/part &+?#/t2";
    const url = new URL(openDiff("turn", path, turn), "https://example.com");
    expect(parseAddress(url)).toEqual({ path, anchor: null, diff: "turn", turn });
  });
});
