import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import planFixture from "@/plan/api/fixtures/files-doc-plan.json";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { FileDocHeading } from "@/plan/files/doc-heading-view";
import { pressureWhen } from "@/plan/files/heading-parts";
import { PlanView } from "@/plan/files/plan/plan-view";

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
  return {
    ...base,
    text,
    sections: [
      { level: 1, title: "Plan", anchor: "plan", start_line: 1, end_line: 3 },
      { level: 2, title: "Pressure tests", anchor: "pressure-tests", start_line: 9, end_line: 11 },
    ],
    plan: { ...base.plan!, metadata_line: 4, ...patch },
  };
}

function renderPlan(doc: DocView): void {
  render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <FileDocHeading doc={doc} />
        <PlanView doc={doc} />
      </MemoryRouter>
    </Provider>,
  );
}

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("PlanView", () => {
  it("splits prose and record around the metadata block with the record's line offset", () => {
    renderPlan(makeDoc());
    const parts = document.querySelectorAll("article .md");
    expect(parts).toHaveLength(2);
    expect(parts[0]!.textContent).toContain("Intro.");
    expect(parts[0]!.textContent).not.toContain("METADATA");
    expect(parts[0]!.querySelector("h1")?.id).toBe("plan");
    expect(parts[1]!.textContent).toContain("Pressure tests");
    expect(parts[1]!.querySelector("h2")?.id).toBe("pressure-tests");
  });

  it("renders no record part when the plan has none", () => {
    const text = TEXT.split("\n").slice(0, 8).join("\n") + "\n";
    renderPlan(makeDoc({}, text));
    expect(document.querySelectorAll("article .md")).toHaveLength(1);
  });

  it("keeps the plan title in the shared heading and suppresses its matching prose H1", () => {
    renderPlan(makeDoc({}, TEXT.replace("# Plan", "# File mode")));
    expect(screen.getAllByRole("heading", { level: 1, name: "File mode" })).toHaveLength(1);
    expect(screen.getByText("Written")).toBeTruthy();
    expect(screen.getByRole("heading", { level: 2, name: /Pressure tests/ }).id).toBe(
      "pressure-tests",
    );
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

  it("shows the parse error as visible text when it carries a bidirectional override", () => {
    renderPlan(makeDoc({ yaml: "stages: [oops", yaml_error: "bad\u202e yaml" }));
    expect(screen.getByRole("alert").textContent).toBe("bad⟨U+202E⟩ yaml");
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

  it("renders a stage description as Markdown, keeping its line breaks", () => {
    const doc = makeDoc();
    doc.plan!.stages[0]!.description =
      "First line.\nSecond line.\n\n| Worker | Tier |\n| --- | --- |\n| W1 | sonnet |";
    renderPlan(doc);
    const row = document.getElementById(`stage:${doc.plan!.stages[0]!.id}`)!;
    fireEvent.click(within(row).getAllByRole("button")[0]!);
    const table = within(row).getByRole("table");
    expect(within(table).getByRole("columnheader", { name: "Worker" })).toBeTruthy();
    expect(within(table).getByRole("cell", { name: "sonnet" })).toBeTruthy();
    expect(within(row).queryByText(/\| --- \|/)).toBeNull();
    expect(row.querySelector(".stage-description p")?.textContent).toBe(
      "First line.\nSecond line.",
    );
  });

  it("shows the pressure date beside the last change", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-10-20T12:00:00Z"));
    const doc = makeDoc();
    renderPlan(doc);
    expect(screen.getByText(`Pressure-tested ${pressureWhen("2026-10-17")}`)).toBeTruthy();
    const changed = screen.getByText("Changed 2 d ago");
    expect(changed).toBeTruthy();
    expect(
      changed.closest("time")?.getAttribute("datetime") ??
        changed.closest("[title]")?.getAttribute("title"),
    ).toBe(doc.plan!.last_change);
  });

  it("links a stage error to its row and leaves a missing Implements target plain", () => {
    renderPlan(makeDoc());
    const toggle = screen.getByRole("button", { name: "1 error, 1 warning" });
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    const link = screen.getByRole("link", { name: "build" });
    expect(link.getAttribute("href")).toBe("#stage:build");
    expect(screen.getByText("Stage needs files", { exact: false })).toBeTruthy();
    const missing = screen.getByText("§07", { exact: false });
    expect(missing.closest("[title]")?.getAttribute("title")).toBe("not found");
    expect(missing.closest("a")).toBeNull();
    expect(document.body.innerHTML).not.toContain("line-through");
    expect(screen.getByRole("link", { name: "§06" })).toBeTruthy();
    fireEvent.click(toggle);
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryByTestId("validation-list")).toBeNull();
  });
});
