import { fireEvent, waitFor, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import filesDocPlan from "@/plan/api/fixtures/files-doc-plan.json";
import type { Sentence } from "@/plan/api/schema-concept";

import { docOf, pageOf, S1, S2, sent, src } from "./reading-column-fixtures";
import {
  active,
  button,
  docs,
  group,
  installColumnTestHooks,
  mount,
  press,
} from "./reading-column-harness";

installColumnTestHooks();

const KEY = "overview/demo/product";
const paragraph = (...sentences: Sentence[]) => ({ kind: "paragraph", sentences });

/** Two paragraphs of two sentences each; every sentence opens to a section of its own. */
function twoParagraphs() {
  return pageOf(KEY, [], {
    blocks: [
      paragraph(
        sent("a", "First of one.", [src(S2, "intro")]),
        sent("b", "Second of one.", [src(S2, "detail")]),
      ),
      paragraph(
        sent("c", "First of two.", [src(S1, "intro")]),
        sent("d", "Second of two.", [src(S1, "detail")]),
      ),
    ],
  });
}

it("draws each paragraph block as its own paragraph", () => {
  const { column } = mount(KEY, twoParagraphs());
  const paragraphs = [...column.querySelectorAll(".concept-paragraph")];
  expect(paragraphs.map((one) => one.textContent)).toEqual([
    expect.stringMatching(/^First of one\..*Second of one\./),
    expect.stringMatching(/^First of two\..*Second of two\./),
  ]);
});

it("the arrows cross a paragraph boundary and Enter, Escape and a work in the second paragraph", async () => {
  const onAttach = vi.fn();
  const { column } = mount(KEY, twoParagraphs(), onAttach);
  column.focus();
  await waitFor(() => expect(active().textContent).toMatch(/^First of one/));
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toMatch(/^Second of one/));
  press("ArrowDown");
  await waitFor(() => expect(active().textContent).toMatch(/^First of two/));

  press("Enter");
  expect(await group(column)).toBeTruthy();
  press("Escape");
  await waitFor(() => expect(within(column).queryByRole("group")).toBeNull());
  expect(active().textContent).toMatch(/^First of two/);

  press("a");
  expect(onAttach).toHaveBeenCalledTimes(1);
  press("ArrowUp");
  await waitFor(() => expect(active().textContent).toMatch(/^Second of one/));
});

it("an opened sentence of the second paragraph stays in its own paragraph", async () => {
  const { column } = mount(KEY, twoParagraphs());
  fireEvent.click(button(column, /^First of two/));
  await group(column);
  const [first, second] = [...column.querySelectorAll(".concept-paragraph")];
  expect(first?.textContent).not.toContain("Alpha text");
  expect(second?.querySelector("[data-passage]")).not.toBeNull();
});

const PLAN = "doc/plans/PLAN-x.md";
const PLAN_TEXT = [
  "# Plan: X",
  "",
  "## Residual risks",
  "Risk one.",
  "",
  "---",
  "",
  "<!-- loom METADATA -->",
  "",
  "```yaml",
  "loom:",
  "  version: 2",
  "```",
  "",
  "<!-- END loom METADATA -->",
].join("\n");

it("a plan's last section opens without the plan's loom metadata block", async () => {
  docs["plans/PLAN-x.md"] = {
    ...docOf(PLAN, PLAN_TEXT),
    sections: [
      { level: 2, title: "Residual risks", anchor: "residual-risks", start_line: 3, end_line: 15 },
    ],
    plan: { ...filesDocPlan.plan, metadata_line: 8 },
  };
  const { column } = mount(
    KEY,
    pageOf(KEY, [sent("r", "The plan names its risks.", [src(PLAN, "residual-risks")])]),
  );

  fireEvent.click(button(column, /^The plan names its risks/));

  expect(await within(column).findByText("Risk one.")).toBeTruthy();
  expect(column.textContent).not.toMatch(/METADATA|loom:|version: 2/);
});
