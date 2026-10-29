import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import hello from "@/plan/api/fixtures/hello.json";
import { checklistViewSchema, type ChecklistView } from "@/plan/api/schema";
import { PaceChoice, paceOffers } from "@/plan/feature/pace";
import { registerOfferAction } from "@/plan/offers/actions";
import { authoringAtom, checklistAtom, youAtom } from "@/plan/state/atoms";

type GuidedKind = "write_section" | "write_plan";

function documentItem(kind: GuidedKind, index: number, feature = "demo") {
  const number = index + 1;
  return {
    step: kind === "write_section" ? 2 : 4,
    text: `Missing document ${number}`,
    path: null,
    offer: {
      id: `${kind}:${number}`,
      kind,
      label: `Draft document ${number}`,
      target: null,
      feature,
      number,
    },
  };
}

function makeChecklist(count: number, kind: GuidedKind = "write_section", feature = "demo") {
  const items = Array.from({ length: count }, (_, index) => documentItem(kind, index, feature));
  return checklistViewSchema.parse({
    feature,
    current: { kind: "step", number: kind === "write_section" ? 2 : 4 },
    steps: [
      { number: 2, name: "Product spec", mark: "gap" },
      { number: 4, name: "Plans", mark: "gap" },
    ],
    primary: items[0]?.offer ?? null,
    secondary: [
      {
        id: "team",
        kind: kind === "write_section" ? "draft_all_sections" : "draft_all_plans",
        label: "Draft every missing document",
        target: null,
        feature,
      },
    ],
    items: [
      ...items,
      { step: 3, text: "Metaplan", path: "doc/plans/demo/METAPLAN.md", offer: null },
      { step: 2, text: "Written section", path: "doc/product/demo/section_09.md", offer: null },
    ],
  });
}

function fixtureOffers(checklist: ChecklistView) {
  const guided = checklist.primary;
  const team = checklist.secondary[0];
  if (!guided || !team) throw new Error("Fixture needs both pace offers");
  return { guided, team };
}

const HOLDER_REASON = `Ada${String.fromCodePoint(0x202e)} holds authoring`;

function renderChoice(
  checklist: ChecklistView,
  readOnly = false,
  activeChecklist: ChecklistView = checklist,
) {
  const store = createStore();
  store.set(checklistAtom, activeChecklist);
  store.set(youAtom, hello.you);
  store.set(authoringAtom, {
    holder: {
      ...hello.you,
      ...(readOnly ? { public_id: "other-browser", name: "Ada\u202E" } : {}),
      away_since: null,
      frees_at: null,
    },
  });
  const step = checklist.steps.find(
    (candidate) =>
      checklist.current.kind === "step" && candidate.number === checklist.current.number,
  );
  if (!step) throw new Error("Fixture needs the current step");
  return render(
    <Provider store={store}>
      <PaceChoice checklist={checklist} step={step} />
    </Provider>,
  );
}

const handlers = {
  write_section: vi.fn(),
  write_plan: vi.fn(),
  draft_all_sections: vi.fn(),
  draft_all_plans: vi.fn(),
};
const unregister: (() => void)[] = [];

beforeEach(() => {
  for (const kind of [
    "write_section",
    "write_plan",
    "draft_all_sections",
    "draft_all_plans",
  ] as const) {
    handlers[kind].mockClear();
    unregister.push(registerOfferAction(kind, handlers[kind]));
  }
});

afterEach(() => {
  cleanup();
  for (const dispose of unregister) dispose();
  unregister.length = 0;
});

it.each(["write_section", "write_plan"] as const)(
  "finds the primary and secondary pace pair for %s",
  (kind) => {
    const checklist = makeChecklist(3, kind);
    const step = checklist.steps.find((candidate) => candidate.number === checklist.items[0]?.step);
    if (!step) throw new Error("Fixture needs the document step");

    expect(paceOffers(checklist, step)).toEqual(fixtureOffers(checklist));
  },
);

it("finds a guided offer on an item when it is absent from primary and secondary", () => {
  const checklist = makeChecklist(3);
  const expected = fixtureOffers(checklist);
  const itemOnly = checklistViewSchema.parse({ ...checklist, primary: null });

  expect(paceOffers(itemOnly, { number: 2, name: "Documents", mark: "gap" })).toEqual(expected);
});

it("finds a primary team offer with a secondary guided offer", () => {
  const checklist = makeChecklist(3);
  const expected = fixtureOffers(checklist);
  const swapped = checklistViewSchema.parse({
    ...checklist,
    primary: expected.team,
    secondary: [expected.guided],
  });

  expect(paceOffers(swapped, { number: 2, name: "Documents", mark: "gap" })).toEqual(expected);
});

it("offers the lowest-numbered missing section even when the items arrive out of order", () => {
  const checklist = makeChecklist(3);
  const expected = fixtureOffers(checklist);
  const reordered = checklistViewSchema.parse({
    ...checklist,
    primary: documentItem("write_section", 2).offer,
    items: [...checklist.items].reverse(),
  });

  expect(paceOffers(reordered, { number: 2, name: "Documents", mark: "gap" })).toEqual(expected);
});

it("finds a team offer on an item for the requested step", () => {
  const checklist = makeChecklist(3);
  const expected = fixtureOffers(checklist);
  const itemOnly = checklistViewSchema.parse({
    ...checklist,
    secondary: [],
    items: [...checklist.items, { step: 2, text: "Draft all", path: null, offer: expected.team }],
  });

  expect(paceOffers(itemOnly, { number: 2, name: "Documents", mark: "gap" })).toEqual(expected);
});

it("ignores a primary guided offer belonging to another step", () => {
  const sections = makeChecklist(3);
  const plans = makeChecklist(3, "write_plan");
  const checklist = checklistViewSchema.parse({
    ...sections,
    secondary: [...sections.secondary, ...plans.secondary],
    items: [...sections.items, ...plans.items],
  });

  expect(paceOffers(checklist, { number: 4, name: "Plans", mark: "gap" })).toEqual(
    fixtureOffers(plans),
  );
});

it.each(["guided", "team", "wrong-team", "other-step"] as const)(
  "returns null when the pace pair is incomplete because of %s",
  (missing) => {
    const base = makeChecklist(3);
    const checklist = checklistViewSchema.parse({
      ...base,
      primary: missing === "guided" ? null : base.primary,
      secondary:
        missing === "team"
          ? []
          : missing === "wrong-team"
            ? makeChecklist(3, "write_plan").secondary
            : base.secondary,
      items: missing === "guided" ? [] : base.items,
    });
    const step = {
      number: missing === "other-step" ? 4 : 2,
      name: "Documents",
      mark: "gap",
    } as const;

    expect(paceOffers(checklist, step)).toBeNull();
  },
);

it.each([
  [1, "Up to 1 teammates, 1 documents, one run."],
  [3, "Up to 3 teammates, 3 documents, one run."],
  [7, "Up to 5 teammates, 7 documents, one run."],
] as const)("shows the cost for %i missing documents", (count, cost) => {
  renderChoice(makeChecklist(count));

  expect(screen.getByText(cost).textContent).toBe(cost);
  expect(screen.getByRole("heading", { name: "Guided" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Draft all, with a team" })).toBeTruthy();
  expect(screen.getAllByRole("button")).toHaveLength(2);
});

it.each(["write_section", "write_plan"] as const)(
  "dispatches both %s pace actions through the registry",
  (kind) => {
    const checklist = makeChecklist(3, kind);
    const { guided, team } = fixtureOffers(checklist);
    renderChoice(checklist);

    fireEvent.click(screen.getByRole("button", { name: guided.label }));
    fireEvent.click(screen.getByRole("button", { name: team.label }));

    expect(handlers[kind]).toHaveBeenCalledWith(guided);
    expect(
      handlers[kind === "write_section" ? "draft_all_sections" : "draft_all_plans"],
    ).toHaveBeenCalledWith(team);
  },
);

it("counts only the plans in the guided offer's step", () => {
  const base = makeChecklist(3, "write_plan");
  const checklist = checklistViewSchema.parse({
    ...base,
    items: [...base.items, ...makeChecklist(7).items],
  });
  renderChoice(checklist);

  expect(screen.getByText("Up to 3 teammates, 3 documents, one run.")).toBeTruthy();
  expect(screen.getByText(/first missing plan/).textContent).toContain("read it");
});

it("renders nothing when no documents are missing", () => {
  const { container } = renderChoice(makeChecklist(0));

  expect(container.innerHTML).toBe("");
});

it("uses the incoming feature checklist before the active checklist catches up", () => {
  const checklist = makeChecklist(3, "write_section", "other");
  const { guided, team } = fixtureOffers(checklist);
  renderChoice(checklist, false, makeChecklist(7));

  expect(screen.getByText("Up to 3 teammates, 3 documents, one run.")).toBeTruthy();
  expect(screen.queryByText("Up to 5 teammates, 7 documents, one run.")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: team.label }));

  expect(handlers.draft_all_sections).toHaveBeenCalledWith(team);
  expect(screen.getByRole("button", { name: guided.label })).toBeTruthy();
});

it("keeps both pace actions disabled with an explanation for a read-only browser", () => {
  const checklist = makeChecklist(3);
  const { guided, team } = fixtureOffers(checklist);
  renderChoice(checklist, true);

  expect(screen.getByRole("heading", { name: "Guided" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Draft all, with a team" })).toBeTruthy();
  const guidedButton = screen.getByRole("button", {
    name: guided.label,
    description: HOLDER_REASON,
  });
  const teamButton = screen.getByRole("button", {
    name: team.label,
    description: HOLDER_REASON,
  });
  expect(guidedButton.getAttribute("aria-disabled")).toBe("true");
  expect(teamButton.getAttribute("aria-disabled")).toBe("true");
  expect(screen.getAllByText(HOLDER_REASON)).toHaveLength(2);
  fireEvent.click(guidedButton);
  fireEvent.click(teamButton);
  expect(handlers.write_section).not.toHaveBeenCalled();
  expect(handlers.draft_all_sections).not.toHaveBeenCalled();
});
