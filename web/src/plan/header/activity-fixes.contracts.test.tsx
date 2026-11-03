import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { getDefaultStore } from "jotai/vanilla";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import headerCss from "@/plan/header/header.css?raw";
import { runStripAtom } from "@/plan/run/use-run";
import { useSummaryFeed } from "@/plan/summaries/state";

import { ActivityPill } from "./activity";

vi.mock("@/plan/run/dashboard-button", () => ({
  DashboardButton: () => <a href="/dashboard">Open dashboard</a>,
}));
vi.mock("@/plan/summaries/state", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/plan/summaries/state")>()),
  useSummaryFeed: vi.fn(),
}));

beforeEach(() => {
  vi.mocked(useSummaryFeed).mockReturnValue({ load: { status: "loading" }, reload: vi.fn() });
});

afterEach(() => {
  cleanup();
  getDefaultStore().set(runStripAtom, null);
  vi.clearAllMocks();
});

type Rule = { at: string[]; selectors: string[]; declarations: Array<[string, string]> };

/** Flat CSS rules with their enclosing at-rules; enough for the plain stylesheets read here. */
function cssRules(css: string): Rule[] {
  const text = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const rules: Rule[] = [];
  const stack: string[] = [];
  let start = 0;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (char === "}") {
      stack.pop();
      start = index + 1;
    } else if (char === "{") {
      const head = (text.slice(start, index).split(";").pop() ?? "").trim();
      if (head.startsWith("@")) {
        stack.push(head);
        start = index + 1;
        continue;
      }
      const end = text.indexOf("}", index);
      const declarations = text
        .slice(index + 1, end)
        .split(";")
        .map((entry) => entry.split(/:(.*)/s).map((part) => part.trim()))
        .filter((pair): pair is [string, string] => pair.length >= 2 && pair[0] !== "")
        .map(([property, value]) => [property.toLowerCase(), value] as [string, string]);
      rules.push({ at: [...stack], selectors: head.split(",").map((s) => s.trim()), declarations });
      index = end;
      start = end + 1;
    }
  }
  return rules;
}

it("a running plan appears once in the activity popover", async () => {
  getDefaultStore().set(runStripAtom, {
    plan: "doc/plans/PLAN-x.md",
    daemon_running: true,
    merged: 2,
    total: 6,
    attention: 0,
    dashboard_url: null,
  });
  render(<ActivityPill compact={false} />);
  fireEvent.click(screen.getByRole("button"));

  const dialog = await screen.findByRole("dialog", { name: "In progress" });
  expect(within(dialog).getAllByText(/PLAN-x\.md/)).toHaveLength(1);
  expect(within(dialog).getAllByText(/2 of 6 stages merged/)).toHaveLength(1);
  const list = within(dialog).getByRole("list", { name: "In progress" });
  expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  expect(within(dialog).queryByText("loom run")).toBeNull();
  expect(within(dialog).queryByRole("progressbar")).toBeNull();
});

it("the planner logo matches the dashboard's sizes", () => {
  const logo = cssRules(headerCss).filter((rule) =>
    rule.selectors.some((selector) => selector.endsWith(".plan-logo")),
  );
  const heights = logo.flatMap((rule) =>
    rule.declarations
      .filter(([property]) => property === "height")
      .map(([, value]) => ({ at: rule.at.join(" "), value })),
  );
  const inside = (query: string) =>
    heights.filter(({ at }) => at.replace(/\s+/g, " ").includes(query)).map(({ value }) => value);

  expect(heights.filter(({ at }) => at === "").map(({ value }) => value)).toContain("32px");
  expect(inside("(min-width: 640px)")).toContain("40px");
  expect(inside("(min-width: 1024px)")).toContain("56px");
  expect(heights.map(({ value }) => value)).not.toContain("22px");
  expect(heights.map(({ value }) => value)).not.toContain("26px");
});
