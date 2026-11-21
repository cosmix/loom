import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { PlanOverviewDialog } from "./plan-overview-dialog";
import { sampleRecord } from "./sample-record";
import { summaryText } from "./summary-text";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

function open() {
  render(<PlanOverviewDialog record={sampleRecord} />);
  return screen.getByRole("alertdialog", { name: /Sample plan/ });
}

function stubClipboard(writeText: (text: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
}

describe("PlanOverviewDialog", () => {
  it("names the plan and the commands that end the lock", () => {
    const dialog = open();
    expect(within(dialog).getByText("Plan complete")).toBeTruthy();
    expect(within(dialog).getByText("loom init")).toBeTruthy();
    expect(within(dialog).getByText("loom clean --state")).toBeTruthy();
    expect(within(dialog).getByText("2 completed · 1 skipped")).toBeTruthy();
    expect(within(dialog).queryByRole("button", { name: /close/i })).toBeNull();
  });

  it("shows the five totals", () => {
    const totals = within(open()).getByRole("group", { name: "Totals" });
    const text = totals.textContent ?? "";
    for (const part of ["1h 00m", "1h 30m", "1.50× parallel", "5.3k", "92.4%", "61.3k"]) {
      expect(text).toContain(part);
    }
    expect(text).toContain("56.0k cache reads");
    expect(text).toContain("across 2 models");
  });

  it("counts model rows, singular for one", () => {
    const one = { ...sampleRecord, models: sampleRecord.models.slice(0, 1) };
    render(<PlanOverviewDialog record={one} />);
    const text = screen.getByRole("group", { name: "Totals" }).textContent ?? "";
    expect(text).toContain("across 1 model");
    expect(text).not.toContain("1 models");
  });

  it("draws the token mix with its legend", () => {
    const dialog = open();
    expect(within(dialog).getByRole("img", { name: "Token mix" }).children).toHaveLength(4);
    const legend = within(dialog).getByRole("list", { name: "Token mix legend" });
    expect(
      within(legend)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Input 1.2k", "Cache write 3.4k", "Output 780", "Cache read 56.0k"]);
  });

  it("draws the timeline with a title per segment, the skipped stub and the colour key", () => {
    const dialog = open();
    const svg = within(dialog).getByRole("img", { name: "Timeline of 3 stages" });
    expect(svg.querySelectorAll("[data-stage]")).toHaveLength(3);
    expect(svg.querySelector('[data-stage="docs"][data-skipped]')).not.toBeNull();
    const phases = [...svg.querySelectorAll("rect[data-phase]")].map((rect) =>
      rect.getAttribute("data-phase"),
    );
    expect(phases).toEqual(["contract", "executing", "executing", "adjudication", "retry"]);
    const titles = [...svg.querySelectorAll("rect[data-phase] > title")].map((t) => t.textContent);
    expect(titles[0]).toMatch(/^Contract writing .+ \(10m\)$/);
    const key = within(dialog).getByRole("list", { name: "Timeline colors" });
    expect(
      within(key)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual([
      "Contract writing",
      "Executing",
      "Dispute and adjudication",
      "Waiting or merging",
      "Retry",
      "Skipped",
    ]);
  });

  it("lists models by fresh tokens with the lane and a share bar key", () => {
    const dialog = open();
    const key = within(dialog).getByRole("list", { name: "Share bar colors" });
    expect(
      within(key)
        .getAllByRole("listitem")
        .map((item) => item.textContent),
    ).toEqual(["Input", "Cache write", "Output"]);
    const rows = within(dialog).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(2);
    expect(rows[0]!.textContent).toContain("opus/high");
    expect(rows[0]!.textContent).toContain("Claude");
    expect(rows[0]!.textContent).toContain("87%");
    expect(rows[1]!.textContent).toContain("gpt-5.6-terra");
    expect(rows[1]!.textContent).toContain("Codex");
  });

  it("says so when no agent reported tokens and when no stage was recorded", () => {
    render(<PlanOverviewDialog record={{ ...sampleRecord, models: [], stages: [] }} />);
    const dialog = screen.getByRole("alertdialog", { name: /Sample plan/ });
    expect(within(dialog).getByText("No agent reported token usage for this plan.")).toBeTruthy();
    expect(within(dialog).getByText("This plan recorded no stages.")).toBeTruthy();
    expect(within(dialog).queryByRole("img", { name: /Timeline of/ })).toBeNull();
    expect(within(dialog).getByText("0 completed").getAttribute("data-tone")).toBe("dimmed");
  });

  it("drops an unused token kind from the mix bar and sizes the rest by count", () => {
    const totals = { ...sampleRecord.totals, output: 0 };
    render(<PlanOverviewDialog record={{ ...sampleRecord, totals }} />);
    const parts = [...screen.getByRole("img", { name: "Token mix" }).children] as HTMLElement[];
    expect(parts.map((part) => part.hasAttribute("data-empty"))).toEqual([
      false,
      false,
      true,
      false,
    ]);
    expect(parts[3]!.style.getPropertyValue("--po-grow")).toBe("56000");
  });

  it("lists each timeline stage as text beside the labelled drawing", () => {
    const dialog = open();
    const list = within(dialog).getByRole("list", { name: "Timeline stages" });
    const items = within(list)
      .getAllByRole("listitem")
      .map((item) => item.textContent);
    expect(items).toHaveLength(3);
    expect(items[0]).toMatch(/^server: \S.*, Contract writing, Executing$/);
    expect(items[1]).toBe("docs: skipped");
    expect(items[2]).toMatch(/^web: \S.*, Executing, Dispute and adjudication, Retry$/);
    expect(list.className).toContain("sr-only");
  });

  it("names the saved file in the footer", () => {
    expect(open().textContent).toContain(`~/.loom/stats/plans/${sampleRecord.plan_id}.json`);
  });

  it("names the saved file under the loom home, which LOOM_HOME may move", () => {
    const text = open().textContent ?? "";
    expect(text).toContain(`<loom home>/stats/plans/${sampleRecord.plan_id}.json`);
    expect(text).toContain("LOOM_HOME");
  });

  it("copies the summary and says so", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    stubClipboard(writeText);
    const dialog = open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy summary" }));
    expect(writeText).toHaveBeenCalledWith(summaryText(sampleRecord));
    expect(await within(dialog).findByRole("button", { name: "Copied" })).toBeTruthy();
  });

  it("says when the copy fails", async () => {
    stubClipboard(() => Promise.reject(new Error("denied")));
    const dialog = open();
    fireEvent.click(within(dialog).getByRole("button", { name: "Copy summary" }));
    expect(await within(dialog).findByRole("button", { name: "Copy failed" })).toBeTruthy();
  });
});
