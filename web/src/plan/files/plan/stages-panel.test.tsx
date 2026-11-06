import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import { docViewSchema, type PlanView, type StageRow } from "@/plan/api/schema-files";
import { StagesPanel } from "@/plan/files/plan/stages-panel";

// React Flow needs layout jsdom lacks; the panel only hands the graph a selection callback.
vi.mock("@/plan/files/plan/stage-graph", () => ({
  StageGraph: ({ onSelect }: { onSelect: (id: string) => void }) => (
    <button type="button" onClick={() => onSelect("dup")}>
      pick dup
    </button>
  ),
}));

function row(id: string, name: string): StageRow {
  const [base] = docViewSchema.parse(planFixture).plan!.stages;
  return { ...base!, id, name, description: `About ${name}` };
}

function planWith(stages: StageRow[]): PlanView {
  return { ...docViewSchema.parse(planFixture).plan!, stages };
}

const LINKS = {
  path: "doc/plans/PLAN-x.md",
  onNavigate: () => {},
  docPaths: new Set<string>(),
  anchorsOf: () => null,
};

const DUPLICATED = [row("dup", "First"), row("solo", "Solo"), row("dup", "Second")];

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("StagesPanel with a repeated stage id", () => {
  it("renders every row without a duplicate key or duplicate DOM id", () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const { container } = render(<StagesPanel plan={planWith(DUPLICATED)} links={LINKS} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    const ids = Array.from(container.querySelectorAll("[id]"), (element) => element.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(error.mock.calls.flat().join(" ")).not.toContain("same key");
  });

  it("keeps the bare id on the first occurrence and suffixes the later one with its index", () => {
    render(<StagesPanel plan={planWith(DUPLICATED)} links={LINKS} />);
    expect(document.getElementById("stage:dup")?.textContent).toContain("First");
    expect(document.getElementById("stage:solo")).not.toBeNull();
    expect(document.getElementById("stage:dup:2")?.textContent).toContain("Second");
  });

  it("points each toggle's aria-controls at its own row body", () => {
    render(<StagesPanel plan={planWith(DUPLICATED)} links={LINKS} />);
    for (const [rowId, text] of [
      ["stage:dup", "About First"],
      ["stage:dup:2", "About Second"],
    ] as const) {
      const li = document.getElementById(rowId)!;
      const toggle = within(li).getByRole("button");
      fireEvent.click(toggle);
      const body = document.getElementById(toggle.getAttribute("aria-controls")!);
      expect(body?.textContent).toContain(text);
      expect(li.contains(body)).toBe(true);
    }
  });

  it("scrolls to the first row of the id when the graph reports it", () => {
    render(<StagesPanel plan={planWith(DUPLICATED)} links={LINKS} />);
    fireEvent.click(screen.getByRole("button", { name: "pick dup" }));
    const mock = vi.mocked(Element.prototype.scrollIntoView);
    expect(mock.mock.instances.at(-1)).toBe(document.getElementById("stage:dup"));
  });
});

describe("StagesPanel YAML toggle", () => {
  it("changes its label without carrying aria-pressed", () => {
    render(<StagesPanel plan={planWith([row("a", "A")])} links={LINKS} />);
    const toggle = screen.getByRole("button", { name: "Show YAML" });
    expect(toggle.hasAttribute("aria-pressed")).toBe(false);
    fireEvent.click(toggle);
    const back = screen.getByRole("button", { name: "Show stages" });
    expect(back.hasAttribute("aria-pressed")).toBe(false);
  });
});
