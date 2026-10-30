import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import type { ChecklistView } from "@/plan/api/schema";
import { atlas, offer, planRow, renderPage } from "@/plan/feature/pages-kit";
import { PressurePage } from "@/plan/feature/pressure-page";
import { registerOfferAction } from "@/plan/offers/actions";

const PATH = "/f/atlas/pressure";
const handler = vi.fn();
let unregister = () => {};

beforeEach(() => {
  handler.mockReset();
  unregister = registerOfferAction("pressure_test", handler);
});

afterEach(() => {
  cleanup();
  unregister();
});

/** `slugs` untested written plans, each with its step-5 offer; `primary` names the checklist's. */
function untestedChecklist(slugs: string[], primary: string | null): ChecklistView {
  const offers = slugs.map((slug) =>
    offer(
      "pressure_test",
      `Pressure-test ${slug}`,
      `doc/plans/atlas/PLAN-${slug}.md`,
      `pt:${slug}`,
    ),
  );
  return atlas({
    plan_rows: slugs.map((slug) => planRow(slug, "written")),
    untested: slugs.length,
    primary: offers.find((candidate) => candidate.id === `pt:${primary}`) ?? null,
    items: offers.map((candidate, index) => ({
      step: 5,
      text: candidate.label,
      path: `doc/plans/atlas/PLAN-${slugs[index]}.md`,
      offer: candidate,
    })),
  });
}

const rowOf = (slug: string) => screen.getByText(slug).closest("li")!;

it("shows a tested plan's date and a Record link, and no button", () => {
  const checklist = atlas({ plan_rows: [planRow("ledger", "written", "2026-10-20")], untested: 0 });
  renderPage(<PressurePage />, PATH, checklist);

  const row = rowOf("ledger");
  expect(within(row).getByText("2026-10-20")).toBeTruthy();
  expect(within(row).queryByText("Not tested")).toBeNull();
  expect(within(row).queryByRole("button")).toBeNull();
  const record = within(row).getByRole("link", { name: "Record" });
  expect(record.getAttribute("href")).toBe("/files/plans/atlas/PLAN-ledger.md");
});

it("draws the primary pressure test once, as the page's primary, and the others as secondary", () => {
  renderPage(<PressurePage />, PATH, untestedChecklist(["gateway", "harbor"], "gateway"));

  const buttons = screen.getAllByRole("button");
  expect(buttons.map((button) => [button.textContent, button.getAttribute("data-look")])).toEqual([
    ["Pressure-test gateway", "primary"],
    ["Pressure-test harbor", "secondary"],
  ]);
  fireEvent.click(within(rowOf("harbor")).getByRole("button"));
  expect(handler).toHaveBeenCalledWith(expect.objectContaining({ id: "pt:harbor" }));
});

it("leaves an untested row without an offer as 'Not tested' with no button", () => {
  const checklist = atlas({
    plan_rows: [planRow("gateway", "done"), planRow("harbor", "written")],
    untested: 2,
  });
  renderPage(<PressurePage />, PATH, checklist);

  expect(within(rowOf("gateway")).getByText("Not tested")).toBeTruthy();
  expect(within(rowOf("harbor")).getByText("Not tested")).toBeTruthy();
  expect(screen.queryByRole("button")).toBeNull();
});

it("shows the offer disabled with one reason while another browser holds authoring", () => {
  renderPage(<PressurePage />, PATH, untestedChecklist(["gateway"], null), { holder: "other" });

  const button = screen.getByRole("button", { name: "Pressure-test gateway" });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText("Ada holds authoring")).toHaveLength(1);
  fireEvent.click(button);
  expect(handler).not.toHaveBeenCalled();
});

it("sends an empty feature to Plans with one sentence", () => {
  renderPage(<PressurePage />, PATH, atlas({ plan_rows: [], untested: 0 }));

  expect(screen.getByRole("heading", { level: 1, name: "Pressure test" })).toBeTruthy();
  expect(screen.getByText(/Write a plan first/)).toBeTruthy();
  expect(screen.getByRole("link", { name: "Go to Plans" }).getAttribute("href")).toBe(
    "/f/atlas/engineering",
  );
  expect(screen.queryByRole("list")).toBeNull();
});

it("links each slug to its plan's concept page", () => {
  const slug = "atlas-01-documents";
  renderPage(<PressurePage />, PATH, atlas({ plan_rows: [planRow(slug, "done", "2026-10-20")] }));

  const link = screen.getByRole("link", { name: slug });
  expect(link.getAttribute("href")).toBe(`/f/atlas/engineering/${slug}`);
});

it("counts the pressure tests from the rows, not from the untested total", () => {
  // `checklist.untested` is 0 here: it covers written plans only, so a running plan never counts.
  const none = atlas({ plan_rows: [planRow("gateway", "running")], untested: 0 });
  const first = renderPage(<PressurePage />, PATH, none);
  expect(screen.getByText("No plan has a pressure test on record.")).toBeTruthy();
  first.unmount();

  const some = atlas({
    plan_rows: [
      planRow("gateway", "done", "2026-10-20"),
      planRow("harbor", "written"),
      planRow("quay", "retired", "2026-09-01"),
    ],
    untested: 1,
  });
  const second = renderPage(<PressurePage />, PATH, some);
  expect(screen.getByText("1 of 2 plans have a pressure test on record.")).toBeTruthy();
  second.unmount();

  const single = atlas({ plan_rows: [planRow("gateway", "done", "2026-10-20")], untested: 0 });
  renderPage(<PressurePage />, PATH, single);
  expect(screen.getByText("1 of 1 plan has a pressure test on record.")).toBeTruthy();
});

it("names the listed plans that have no file yet under the rows", () => {
  const checklist = atlas({
    plan_rows: [planRow("gateway", "written")],
    plans: { listed: 13, total: 1, written: 1, running: 0, done: 0 },
  });
  renderPage(<PressurePage />, PATH, checklist);

  expect(screen.getByText("12 listed plans are not written yet.")).toBeTruthy();
  expect(screen.getByRole("link", { name: "Go to Plans" }).getAttribute("href")).toBe(
    "/f/atlas/engineering",
  );
});
