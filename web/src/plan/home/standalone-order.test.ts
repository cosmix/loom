import { describe, expect, it } from "vitest";

import type { StandalonePlanView } from "@/plan/api/schema";
import {
  activeStates,
  DEFAULT_SORT,
  firstDirection,
  modifiedLabel,
  shortTitle,
  shownPlans,
  stateCounts,
  toggleState,
} from "@/plan/home/standalone-order";

function plan(
  slug: string,
  title: string,
  state: StandalonePlanView["state"],
  last_change: string | null,
): StandalonePlanView {
  return { slug, title, state, last_change, has_summary: false, path: `doc/plans/PLAN-${slug}.md` };
}

const plans = [
  plan("b", "Plan: Beta", "done", "2026-11-01T00:00:00Z"),
  plan("none", "No time", "written", null),
  plan("a", "Alpha", "running", "2026-11-03T00:00:00Z"),
  plan("bad", "Bad time", "written", "not a date"),
  plan("c", "Gamma", "written", "2026-10-02T00:00:00Z"),
];
const slugs = (list: StandalonePlanView[]) => list.map((item) => item.slug);

describe("shownPlans", () => {
  it("defaults to newest first, with unreadable times last by title", () => {
    expect(DEFAULT_SORT).toEqual({ key: "modified", direction: "descending" });
    expect(slugs(shownPlans(plans, "", DEFAULT_SORT))).toEqual(["a", "b", "c", "bad", "none"]);
  });

  it("keeps unreadable times last when the oldest come first", () => {
    const sort = { key: "modified", direction: "ascending" } as const;
    expect(slugs(shownPlans(plans, "", sort))).toEqual(["c", "b", "a", "bad", "none"]);
  });

  it("sorts titles without their Plan: prefix and filters by the full title or slug", () => {
    const sort = { key: "title", direction: "ascending" } as const;
    expect(slugs(shownPlans(plans, "", sort))).toEqual(["a", "bad", "b", "c", "none"]);
    expect(slugs(shownPlans(plans, "plan: beta", sort))).toEqual(["b"]);
  });

  it("does not reorder the list it was given", () => {
    const before = slugs(plans);
    shownPlans(plans, "", DEFAULT_SORT);
    expect(slugs(plans)).toEqual(before);
  });
});

describe("state filter", () => {
  it("counts the states that occur, running first", () => {
    expect(stateCounts(plans)).toEqual([
      { state: "running", count: 1 },
      { state: "written", count: 3 },
      { state: "done", count: 1 },
    ]);
    expect(stateCounts([])).toEqual([]);
  });

  it("toggles a state in and out without touching the set it was given", () => {
    const none: ReadonlySet<StandalonePlanView["state"]> = new Set();
    const running = toggleState(none, "running");
    expect([...running]).toEqual(["running"]);
    expect([...toggleState(toggleState(running, "done"), "running")]).toEqual(["done"]);
    expect(none.size).toBe(0);
  });

  it("drops a picked state the list no longer holds", () => {
    const counts = stateCounts(plans);
    expect([...activeStates(new Set(["retired", "done"]), counts)]).toEqual(["done"]);
    expect(activeStates(new Set(["retired"]), counts).size).toBe(0);
  });

  it("filters by state first, then by the query, then sorts", () => {
    const written = new Set(["written"] as const);
    expect(slugs(shownPlans(plans, "", DEFAULT_SORT, written))).toEqual(["c", "bad", "none"]);
    expect(slugs(shownPlans(plans, "time", DEFAULT_SORT, written))).toEqual(["bad", "none"]);
    const two = new Set(["running", "done"] as const);
    expect(slugs(shownPlans(plans, "", DEFAULT_SORT, two))).toEqual(["a", "b"]);
    expect(shownPlans(plans, "alpha", DEFAULT_SORT, new Set(["done"]))).toEqual([]);
  });
});

describe("firstDirection", () => {
  it("starts dates newest first and words from A", () => {
    expect(firstDirection("modified")).toBe("descending");
    expect(firstDirection("title")).toBe("ascending");
    expect(firstDirection("state")).toBe("ascending");
  });
});

describe("shortTitle", () => {
  it("drops a leading Plan: and keeps a title that is nothing else", () => {
    expect(shortTitle("Plan: Fix search")).toBe("Fix search");
    expect(shortTitle("plan:Fix")).toBe("Fix");
    expect(shortTitle("Planning trace")).toBe("Planning trace");
    expect(shortTitle("Plan:")).toBe("Plan:");
  });
});

describe("modifiedLabel", () => {
  const now = Date.parse("2026-11-06T12:00:00Z");

  it("is relative within a week, with the full time as its title", () => {
    const label = modifiedLabel("2026-11-06T09:00:00Z", now);
    expect(label?.text).toBe("3 h ago");
    expect(label?.title).toContain("2026");
  });

  it("is a short date after a week, with the year only for another year", () => {
    expect(modifiedLabel("2026-10-01T12:00:00Z", now)?.text).not.toContain("2026");
    expect(modifiedLabel("2025-10-01T12:00:00Z", now)?.text).toContain("2025");
  });

  it("is null for a missing or unreadable time", () => {
    expect(modifiedLabel(null, now)).toBeNull();
    expect(modifiedLabel("soon", now)).toBeNull();
  });
});
