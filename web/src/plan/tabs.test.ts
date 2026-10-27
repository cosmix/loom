import { describe, expect, it } from "vitest";
import { PLAN_TABS, PLAN_TAB_ROUTES } from "./tabs";

describe("plan tabs", () => {
  it("starts with no registered tabs", () => {
    expect(PLAN_TABS).toEqual([]);
    expect(PLAN_TAB_ROUTES).toEqual([]);
  });
});
