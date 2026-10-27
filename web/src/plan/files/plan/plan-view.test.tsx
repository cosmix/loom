import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { PlanView } from "@/plan/files/plan/plan-view";

vi.mock("@/plan/markdown", () => ({
  Markdown: (props: { source: string; lineOffset?: number }) => (
    <div data-testid="markdown" data-offset={props.lineOffset ?? 0}>
      {props.source}
    </div>
  ),
}));

const TEXT = [
  "# Plan",
  "",
  "Intro.",
  "<!-- loom METADATA -->",
  "```yaml",
  "stages: []",
  "```",
  "<!-- END loom METADATA -->",
  "## Pressure tests",
  "Record body",
  "",
].join("\n");

function makeDoc(patch: Partial<NonNullable<DocView["plan"]>> = {}, text = TEXT): DocView {
  const base = docViewSchema.parse(planFixture);
  return { ...base, text, plan: { ...base.plan!, metadata_line: 4, ...patch } };
}

function renderPlan(doc: DocView): void {
  render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <PlanView doc={doc} />
      </MemoryRouter>
    </Provider>,
  );
}

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(cleanup);

describe("PlanView", () => {
  it("splits prose and record around the metadata block with the record's line offset", () => {
    renderPlan(makeDoc());
    const parts = screen.getAllByTestId("markdown");
    expect(parts).toHaveLength(2);
    expect(parts[0]!.textContent).toContain("Intro.");
    expect(parts[0]!.textContent).not.toContain("METADATA");
    expect(parts[0]!.getAttribute("data-offset")).toBe("0");
    expect(parts[1]!.textContent).toContain("## Pressure tests");
    expect(parts[1]!.getAttribute("data-offset")).toBe("8");
  });

  it("renders no record part when the plan has none", () => {
    const text = TEXT.split("\n").slice(0, 8).join("\n") + "\n";
    renderPlan(makeDoc({}, text));
    expect(screen.getAllByTestId("markdown")).toHaveLength(1);
  });

  it("shows one line when the metadata block is missing", () => {
    renderPlan(makeDoc({ yaml: null, stages: [], metadata_line: null }));
    expect(screen.getByText("No metadata block yet.")).toBeTruthy();
  });

  it("shows the parse error and the raw block", () => {
    renderPlan(makeDoc({ yaml: "stages: [oops", yaml_error: "bad yaml at line 1" }));
    expect(screen.getByRole("alert").textContent).toBe("bad yaml at line 1");
    expect(screen.getByText("stages: [oops")).toBeTruthy();
  });

  it("toggles Show YAML", () => {
    renderPlan(makeDoc());
    expect(screen.queryByText(/- id: build/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show YAML" }));
    expect(screen.getByText(/- id: build/)).toBeTruthy();
    expect(document.getElementById("stage:build")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show stages" }));
    expect(document.getElementById("stage:build")).not.toBeNull();
  });

  it("scrolls to the stage row when a graph node is clicked", () => {
    renderPlan(makeDoc());
    const graph = screen.getByTestId("stage-graph");
    fireEvent.click(within(graph).getByText("Review"));
    const row = document.getElementById("stage:review")!;
    expect(Element.prototype.scrollIntoView).toHaveBeenCalled();
    const mock = vi.mocked(Element.prototype.scrollIntoView);
    expect(mock.mock.instances.at(-1)).toBe(row);
  });

  it("expands a stage row to description, acceptance and files", () => {
    renderPlan(makeDoc());
    const row = document.getElementById("stage:build")!;
    expect(within(row).queryByText("Implement it.")).toBeNull();
    fireEvent.click(within(row).getByRole("button"));
    expect(within(row).getByText("Implement it.")).toBeTruthy();
    expect(within(row).getByText("It works")).toBeTruthy();
    expect(within(row).getByText("loom/src/lib.rs")).toBeTruthy();
  });

  it("shows the pressure date beside the last change", () => {
    renderPlan(makeDoc());
    expect(screen.getByText("Pressure tested 2026-10-17")).toBeTruthy();
    expect(screen.getByText("Last change 2026-10-18")).toBeTruthy();
  });

  it("links a stage error to its row and strikes through a missing Implements target", () => {
    renderPlan(makeDoc());
    fireEvent.click(screen.getByRole("button", { name: "1 error, 1 warning" }));
    const link = screen.getByRole("link", { name: "build" });
    expect(link.getAttribute("href")).toBe("#stage:build");
    expect(screen.getByText("Stage needs files", { exact: false })).toBeTruthy();
    const missing = screen.getByText("Section 7");
    expect(missing.tagName).toBe("S");
    expect(screen.getByRole("link", { name: "Section 6" })).toBeTruthy();
  });
});
