import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it } from "vitest";

import { DocHeading, type DocHeadingProps, ToolbarGroup } from "./doc-heading";
import headingSheet from "./doc-heading.css?raw";

afterEach(cleanup);

function renderHeading(props: Partial<DocHeadingProps> = {}) {
  return render(<DocHeading breadcrumb={[]} kind="plan" title="Execution handoff" {...props} />, {
    wrapper: MemoryRouter,
  });
}

it("sets the header as one grid of rows over a hairline, in the page's own sheet", () => {
  const { container } = renderHeading();

  const header = container.querySelector("header") as HTMLElement;
  expect(header.className).toBe("doc-heading");
  const css = headingSheet.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
  expect(css).toContain(
    ".doc-heading { container-type: inline-size; display: grid; row-gap: var(--space-3);",
  );
  expect(css).toContain("border-bottom: 1px solid var(--hairline); background: var(--background);");
});

it("renders a crumb icon and sets a plan's slug crumb in mono", () => {
  renderHeading({
    breadcrumb: [
      { label: "Demo feature", to: "/f/demo", icon: <svg data-testid="crumb-icon" /> },
      { label: "Plans" },
      { label: "feat-planner-12", mono: true },
    ],
  });

  const nav = screen.getByRole("navigation", { name: "Breadcrumb" });
  expect(screen.getByTestId("crumb-icon").parentElement?.getAttribute("aria-hidden")).toBe("true");
  expect(within(nav).getByRole("link", { name: "Demo feature" }).getAttribute("href")).toBe(
    "/f/demo",
  );
  expect(within(nav).getByText("feat-planner-12").closest("[aria-current]")?.className).toContain(
    "font-mono",
  );
  expect(within(nav).getByText("Plans").closest("[aria-current]")).toBeNull();
});

it("puts an icon in each segment of the view switch", () => {
  renderHeading({ views: { current: "summary", summaryTo: "/summary", sourceTo: "/source" } });

  const group = screen.getByRole("group", { name: "View" });
  expect(group.querySelector("svg.lucide-sparkles")).not.toBeNull();
  expect(group.querySelector("svg.lucide-file-text")).not.toBeNull();
  expect(group.className).not.toContain("border");
});

it("sets the actions in a toolbar row under the title, whose empty groups collapse", () => {
  renderHeading({
    actions: (
      <>
        <ToolbarGroup>{null}</ToolbarGroup>
        <ToolbarGroup>
          <button type="button">Discuss</button>
        </ToolbarGroup>
      </>
    ),
  });

  const group = screen.getByRole("button", { name: "Discuss" }).parentElement as HTMLElement;
  expect(group.className).toBe("doc-heading-group");
  const toolbar = group.parentElement as HTMLElement;
  expect(toolbar.className).toBe("doc-heading-toolbar");
  expect(toolbar.previousElementSibling).toBe(
    screen.getByRole("heading", { level: 1 }).parentElement,
  );
  expect(toolbar.firstElementChild?.childElementCount).toBe(0);
  const css = headingSheet.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
  expect(css).toContain(".doc-heading-group:empty { display: none; }");
  expect(css).toContain(".doc-heading-toolbar { display: flex; flex-wrap: wrap;");
  // Under 40rem of heading width the bar stacks its groups as rows divided by hairlines.
  expect(css).toContain(
    "@container (max-width: 40rem) { .doc-heading-toolbar { flex-direction: column; align-items: stretch; } .doc-heading-group + .doc-heading-group { border-inline-start: 0; border-top: 1px solid var(--hairline); } }",
  );
});

it("sets a status row's action at its end and its detail under the words, and facts on their own line", () => {
  renderHeading({
    chips: [
      {
        key: "status",
        label: "Out of date",
        tone: "warning",
        action: <button type="button">Rebuild all</button>,
        detail: "21 summaries out of date.",
      },
      { key: "built", label: "Built by", kind: "fact", links: [{ label: "05", to: "/plans/a" }] },
    ],
  });

  const row = screen.getByText("Out of date").closest(".doc-heading-row") as HTMLElement;
  expect(row.hasAttribute("data-action")).toBe(true);
  expect(row.querySelector(".doc-heading-row__detail")?.textContent).toBe(
    "21 summaries out of date.",
  );
  expect(row.lastElementChild).toBe(
    screen.getByRole("button", { name: "Rebuild all" }).parentElement,
  );
  const facts = screen.getByText("Built by").closest(".doc-heading-facts") as HTMLElement;
  expect(facts.tagName).toBe("DL");
  expect(facts.previousElementSibling).toBe(screen.getByRole("list", { name: "Status" }));
  expect(screen.getByRole("link", { name: "05" }).closest("dd")).not.toBeNull();
  const css = headingSheet.replace(/\/\*[\s\S]*?\*\//g, "").replace(/\s+/g, " ");
  expect(css).toContain(
    "@container (max-width: 30rem) { .doc-heading-row { grid-template-columns: minmax(0, 1fr); }",
  );
});

it.each([
  ["accent", "--tone-executing"],
  ["warning", "--tone-warning"],
  ["danger", "--tone-blocked"],
] as const)("tints only the icon of a %s chip and draws no fill", (tone, variable) => {
  renderHeading({
    chips: [{ key: "t", label: "Chip", tone, icon: <svg data-testid="icon" /> }],
  });

  const chip = screen.getByText("Chip").closest("[data-chip]") as HTMLElement;
  expect(chip.getAttribute("data-tone")).toBe(tone);
  expect(chip.className).not.toMatch(/\bbg-/);
  expect(screen.getByTestId("icon").parentElement?.className).toContain(`text-(${variable})`);
  expect(chip.className).toContain("doc-heading-chip");
});

it("leaves neutral and ok chips unfilled", () => {
  renderHeading({
    chips: [
      { key: "n", label: "Neutral" },
      { key: "o", label: "Fine", tone: "ok", icon: <svg data-testid="ok-icon" /> },
    ],
  });

  expect(screen.getByText("Neutral").closest("[data-chip]")?.className).not.toMatch(/\bbg-/);
  expect(screen.getByText("Fine").closest("[data-chip]")?.className).not.toMatch(/\bbg-/);
  expect(screen.getByTestId("ok-icon").parentElement?.className).toContain(
    "text-(--tone-completed)",
  );
});

it("renders chip links after the label, plain when a target is missing", () => {
  renderHeading({
    chips: [
      {
        key: "implements",
        label: "Implements",
        links: [
          { label: "§05", to: "/product/5", title: "Concept mode" },
          { label: "§99", title: "not found", description: "not found" },
        ],
      },
      {
        key: "built",
        label: "Built by",
        linksSeparator: " · ",
        links: [
          { label: "05", to: "/plans/a" },
          { label: "09", to: "/plans/b" },
        ],
      },
    ],
  });

  const implemented = screen.getByText("Implements", { exact: false }).closest("[data-chip]");
  expect(implemented?.textContent).toBe("Implements §05, §99 (not found)");
  expect(screen.getByRole("link", { name: "§05" }).getAttribute("title")).toBe("Concept mode");
  const missing = screen.getByText("§99", { exact: false });
  expect(missing.closest("a")).toBeNull();
  expect(missing.closest("[title]")?.getAttribute("title")).toBe("not found");
  expect(screen.getByText("Built by", { exact: false }).closest("[data-chip]")?.textContent).toBe(
    "Built by 05 · 09",
  );
  expect(screen.getByRole("link", { name: "09" }).getAttribute("href")).toBe("/plans/b");
});
