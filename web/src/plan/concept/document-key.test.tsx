import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import overviewFixture from "@/plan/api/fixtures/concept-overview.json";
import summaryFixture from "@/plan/api/fixtures/concept-summary.json";
import { conceptPageSchema, type ConceptPage, type KeyChip } from "@/plan/api/schema-concept";

import { chipTitle, DocumentKey } from "./document-key";
import { conceptPageAtom } from "./page-state";

afterEach(cleanup);

function chipOf(extra: Partial<KeyChip> = {}): KeyChip {
  return {
    doc: {
      id: "section_01",
      title: "01 — Planning",
      path: "doc/product/demo/section_01.md",
      page: "demo/product/section_01",
    },
    colour: 3,
    label: "01",
    out_of_date: null,
    no_plan: false,
    ...extra,
  };
}

function showChip(chip: KeyChip, loaded?: ConceptPage) {
  const page = conceptPageSchema.parse({ ...overviewFixture, key_chips: [chip] });
  const store = createStore();
  if (loaded) {
    store.set(conceptPageAtom(chip.doc.page), { page: loaded, previous: null, error: null });
  }
  render(
    <Provider store={store}>
      <MemoryRouter>
        <TooltipProvider>
          <DocumentKey page={page} pageKey={page.key} />
        </TooltipProvider>
      </MemoryRouter>
    </Provider>,
  );
  return screen.getByRole("link");
}

describe("chipTitle", () => {
  it.each([
    ["01 — Planning", "Planning"],
    ["2 – Review", "Review"],
    ["3 - Release", "Release"],
    ["  04  —  Planning", "Planning"],
    ["Planning", "Planning"],
    ["3-D rendering", "3-D rendering"],
    ["Workflow 2 — Model", "Workflow 2 — Model"],
  ])("formats %j as %j", (title, expected) => {
    expect(chipTitle(title)).toBe(expected);
  });
});

it("links a key chip to its feature-qualified summary", () => {
  const link = showChip(chipOf());

  expect(link.getAttribute("href")).toBe("/f/demo/product/section_01");
});

it("shows a format character in the chip's label and title as its code point", () => {
  const chip = chipOf({ label: "0\u202E1" });
  chip.doc.title = "01 — Plan\u202Ening";
  const link = showChip(chip);

  expect(link.textContent).toBe("0⟨U+202E⟩1Plan⟨U+202E⟩ning");
});

it("renders a section DocMark in the chip's colour", () => {
  const link = showChip(chipOf());
  const mark = link.querySelector<HTMLElement>('[data-kind="section"]');

  expect(mark?.getAttribute("data-rebuilding")).toBe("false");
  expect(mark?.style.color).toBe("var(--doc-3)");
  // The 16% light / 20% dark tint is the `.concept-key-mark` rule, fed by this variable.
  expect(mark?.classList.contains("concept-key-mark")).toBe(true);
  expect(mark?.style.getPropertyValue("--key-colour")).toBe("var(--doc-3)");
  expect(mark?.firstElementChild?.classList.contains("size-5")).toBe(true);
  expect(mark?.firstElementChild?.classList.contains("!text-inherit")).toBe(true);
  expect(mark?.firstElementChild?.classList.contains("!bg-transparent")).toBe(true);
  expect(mark?.querySelector(".lucide-book-open")).not.toBeNull();
});

it.each([
  ["overview/demo/product", "vision"],
  ["overview/demo/engineering", "metaplan"],
  ["demo/engineering/feature-x", "plan"],
  ["plans/feature-x", "plan"],
])("derives the DocMark kind for %s", (page, kind) => {
  const chip = chipOf();
  chip.doc.page = page;
  const link = showChip(chip);

  expect(link.querySelector(`[data-kind="${kind}"]`)).not.toBeNull();
});

it("shows turning summary activity when the chip is rebuilding", () => {
  const link = showChip(chipOf({ out_of_date: { reason: "rebuilding", detail: null } }));
  const mark = link.querySelector('[data-kind="section"][data-rebuilding="true"]');

  expect(
    mark?.querySelector('[data-kind="summaries"] .lucide-refresh-cw.activity-glyph__motion'),
  ).not.toBeNull();
  expect(mark?.querySelector(".lucide-book-open")).toBeNull();
});

it("shows turning summary activity from the loaded summary's state", () => {
  const loaded = conceptPageSchema.parse({
    ...summaryFixture,
    state: { state: "rebuilding", out_of_date: null },
  });
  const link = showChip(chipOf(), loaded);

  expect(link.querySelector('[data-rebuilding="true"] [data-kind="summaries"]')).not.toBeNull();
});

it("renders inline Markdown in titles after the mono number without nested links", () => {
  const chip = chipOf({ label: "02" });
  chip.doc.title = "02 — `loom plan` *Command* and **Server** [guide](https://example.com)";
  const link = showChip(chip);

  expect(link.querySelector(".font-mono")?.textContent).toBe("02");
  expect(link.querySelector("code")?.textContent).toBe("loom plan");
  expect(link.querySelector("em")?.textContent).toBe("Command");
  expect(link.querySelector("strong")?.textContent).toBe("Server");
  expect(link.textContent).not.toContain("—");
  expect(link.textContent).not.toContain("`");
  expect(link.textContent).toContain("guide");
  expect(screen.getAllByRole("link")).toHaveLength(1);
});

it("keeps implements text in the description for a feature-qualified plan chip", () => {
  const chip = chipOf();
  chip.doc.page = "demo/engineering/feature-x";
  const loaded = conceptPageSchema.parse({
    ...summaryFixture,
    key: chip.doc.page,
    implements: { kind: "not_stated" },
  });
  const link = showChip(chip, loaded);
  const description = document.getElementById(link.getAttribute("aria-describedby") ?? "");

  expect(description?.textContent).toBe("Implements: not stated");
});

describe("long keys", () => {
  function showKey(count: number) {
    const chips = Array.from({ length: count }, (_, index) => {
      const label = String(index + 1).padStart(2, "0");
      const chip = chipOf({ label, colour: index + 1 });
      chip.doc = { ...chip.doc, id: `section_${label}`, page: `demo/product/section_${label}` };
      return chip;
    });
    const page = conceptPageSchema.parse({ ...overviewFixture, key_chips: chips });
    render(
      <Provider store={createStore()}>
        <MemoryRouter>
          <TooltipProvider>
            <DocumentKey page={page} pageKey={page.key} />
          </TooltipProvider>
        </MemoryRouter>
      </Provider>,
    );
  }

  it("shows four chips, then expands the rest in place and folds them back", () => {
    showKey(6);
    expect(screen.getAllByRole("link")).toHaveLength(4);

    fireEvent.click(screen.getByRole("button", { name: /^…\s?2 more$/ }));
    expect(screen.getAllByRole("link")).toHaveLength(6);

    fireEvent.click(screen.getByRole("button", { name: "Fewer" }));
    expect(screen.getAllByRole("link")).toHaveLength(4);
  });

  it("has no more chip and no Fold all button for a short key", () => {
    showKey(4);
    expect(screen.getAllByRole("link")).toHaveLength(4);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
