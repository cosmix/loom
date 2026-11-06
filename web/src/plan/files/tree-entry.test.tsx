import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { MemoryRouter } from "react-router";
import { afterEach, expect, it, vi } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { fileTreeSchema, type TreeEntry } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { FileMode } from "@/plan/files/file-mode";
import { gapAddress, plainTitle, TreeEntryRow } from "@/plan/files/tree-entry";
import { registerOfferAction } from "@/plan/offers/actions";
import { activeFeatureAtom, projectAtom } from "@/plan/state/atoms";

vi.mock("@/plan/files/api", () => ({
  fetchDoc: vi.fn(),
  fetchTree: vi.fn(),
  searchDocs: vi.fn(),
  useFileSocket: () => {},
}));
vi.mock("@/plan/files/plan/plan-view", () => ({ PlanView: () => null }));

function renderEntry(fields: Partial<TreeEntry> = {}) {
  const entry: TreeEntry = {
    feature: "feature",
    misplaced: false,
    path: "doc/plans/feature/PLAN-one.md",
    target: null,
    title: "One plan",
    file_name: "PLAN-one.md",
    kind: "plan",
    state: "written",
    locked: false,
    gap: false,
    pressure_date: null,
    errors: 0,
    blurb: null,
    children: [],
    ...fields,
  };
  return render(
    <Provider store={createStore()}>
      <MemoryRouter>
        <ul>
          <TreeEntryRow entry={entry} forceOpen={false} />
        </ul>
      </MemoryRouter>
    </Provider>,
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

it("marks a plan with its DocMark and says only a running plan's state aloud", () => {
  const { container } = renderEntry({ state: "running" });

  const mark = container.querySelector(".lucide-list-checks")?.parentElement;
  expect(mark?.classList.contains("size-4")).toBe(true);
  expect(mark?.getAttribute("aria-hidden")).toBe("true");
  expect(mark?.getAttribute("style")).toContain("var(--doc-5)");
  // A running plan draws the live status dot beside its visible word.
  const running = screen.getByText("running");
  expect(running.classList.contains("sr-only")).toBe(false);
  expect(
    running.closest(".plan-tree__signal")?.querySelector(".activity-glyph__led"),
  ).not.toBeNull();
});

it.each([
  ["written", "gauge", "Not pressure-tested"],
  ["retired", "archive", "retired"],
] as const)(
  "draws a %s plan's sign as a quiet glyph with its word on hover",
  (state, glyph, word) => {
    const { container } = renderEntry({ state });

    const icon = container.querySelector(`.plan-tree__signals .lucide-${glyph}`);
    expect(icon?.parentElement?.getAttribute("title")).toBe(word);
    expect(icon?.parentElement?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.getByText(word).classList.contains("sr-only")).toBe(true);
  },
);

it("draws nothing for a done plan but keeps its state and lock for assistive technology", () => {
  const { container } = renderEntry({ state: "done", locked: true });

  expect(screen.getByText("done").classList.contains("sr-only")).toBe(true);
  expect(screen.getByText("Locked").classList.contains("sr-only")).toBe(true);
  expect(container.querySelector(".plan-tree__signals svg")).toBeNull();
  expect(container.querySelector(".lucide-lock")).toBeNull();
});

it("shows a tested plan's flask in the done tone with its date", () => {
  const { container } = renderEntry({ pressure_date: "2026-10-17" });

  const flask = container.querySelector(".lucide-flask-conical")?.parentElement;
  expect(flask?.getAttribute("data-tone")).toBe("done");
  expect(flask?.getAttribute("title")).toBe("Pressure-tested 2026-10-17");
});

it("keeps the row on one line: the short title truncates, the signals keep their width", () => {
  renderEntry({
    state: "running",
    errors: 2,
    title: `Plan: ${"A very long plan title ".repeat(8)}`,
  });

  const link = screen.getByRole("link");
  const row = link.parentElement as HTMLElement;
  const title = link.querySelector(".plan-tree__title") as HTMLElement;
  const signals = row.querySelector(".plan-tree__signals") as HTMLElement;
  expect(row.classList.contains("plan-tree__row")).toBe(true);
  expect(title.textContent?.startsWith("A very long")).toBe(true);
  expect(link.getAttribute("title")?.startsWith("Plan: A very long")).toBe(true);
  expect(signals.parentElement).toBe(row);
  expect([...row.children].indexOf(link)).toBeLessThan([...row.children].indexOf(signals));
  expect(within(signals).getByText("2 errors")).toBeTruthy();
  expect(within(signals).getByText("running")).toBeTruthy();
});

it("sets a feature plan's ordinal in its own span before the short title", () => {
  renderEntry({ title: "Plan: Server core", file_name: "PLAN-feature-02-server.md" });

  const link = screen.getByRole("link");
  expect(link.querySelector(".plan-tree__number")?.textContent).toBe("02");
  expect(link.querySelector(".plan-tree__title")?.textContent).toBe("Server core");
  expect(link.textContent).toBe("02 Server core");
});

it.each([
  ["vision", "telescope"],
  ["section", "book-open"],
  ["note", "sticky-note"],
  ["metaplan", "map"],
  ["brief", "file-text"],
  ["knowledge", "library"],
  ["review", "file-search"],
] as const)("shows the document mark for a %s entry", (kind, glyph) => {
  const { container } = renderEntry({ kind, state: null });

  const mark = container.querySelector(`.lucide-${glyph}`)?.parentElement;
  expect(mark?.classList.contains("size-4")).toBe(true);
});

it.each([
  ["document", {}],
  ["gap", { gap: true, path: null }],
  ["group", { path: null }],
] as const)("renders inline markup in a %s title without adding links", (label, fields) => {
  const title = "Use `Cache` with *care* and **tests** [guide](https://example.com)";
  const { container } = renderEntry({ title, ...fields });

  expect(container.querySelector("code")?.textContent).toBe("Cache");
  expect(container.querySelector("em")?.textContent).toBe("care");
  expect(container.querySelector("strong")?.textContent).toBe("tests");
  expect(container.textContent).not.toContain("`");
  expect(container.querySelectorAll("a")).toHaveLength(label === "group" ? 0 : 1);
  expect(container.querySelector("a a")).toBeNull();
  if (label !== "group") {
    expect(screen.getByRole("link").getAttribute("href")).toBe(
      label === "gap" ? gapAddress(title, "feature") : "/files/plans/feature/PLAN-one.md",
    );
  }
});

it("renders bidi controls visibly in isolated titles and expand labels", () => {
  const { container } = renderEntry({
    title: "Review **safe**\u202e",
    children: [{ ...fileTreeSchema.parse(treeFixture).product[0]!, children: [] }],
  });

  const link = screen.getByRole("link", { name: "Review safe⟨U+202E⟩" });
  const title = link.querySelector<HTMLElement>(".plan-tree__title");
  expect(link.textContent).toBe("Review safe⟨U+202E⟩");
  expect(link.querySelector("strong")?.textContent).toBe("safe");
  expect(title?.style.unicodeBidi).toBe("isolate");
  expect(title?.style.direction).toBe("ltr");
  expect(screen.getByRole("button", { name: "Expand Review safe⟨U+202E⟩" })).toBeTruthy();
  expect(container.textContent).not.toContain("\u202e");
});

it("renders bidi controls visibly in the tooltip's title and file name and in the blurb", () => {
  const { container } = renderEntry({
    title: "One‮ plan",
    file_name: "PLAN‮-one.md",
    blurb: "Plan‮ blurb",
  });

  expect(screen.getByRole("link").getAttribute("title")).toBe(
    "One⟨U+202E⟩ plan\nPLAN⟨U+202E⟩-one.md",
  );
  expect(screen.getByText("Plan⟨U+202E⟩ blurb")).toBeTruthy();
  expect(container.textContent).not.toContain("‮");
});

it.each([
  ["`loom plan` **Engineering** *Metaplan*", "loom plan Engineering Metaplan"],
  ["Use __strong__ and _emphasis_", "Use strong and emphasis"],
  ["Use `snake_case_*` and ``literal`tick``", "Use snake_case_* and literal`tick"],
  ["Keep snake_case and a lone * marker", "Keep snake_case and a lone * marker"],
  ["**Use `hooks/`** with [care](https://example.com)", "Use hooks/ with care"],
])("projects the inline title %s to plain text", (title, expected) => {
  expect(plainTitle(title)).toBe(expected);
});

it("starts a row with children closed and names its toggle with the full title", () => {
  renderEntry({
    title: "`loom plan` **Engineering** *Metaplan*",
    kind: "metaplan",
    state: null,
    children: [{ ...fileTreeSchema.parse(treeFixture).product[0]!, children: [] }],
  });

  // The row shows "Metaplan"; the toggle says which metaplan in full.
  expect(screen.getByRole("link").textContent).toBe("Metaplan");
  const button = screen.getByRole("button", { name: "Expand loom plan Engineering Metaplan" });
  expect(button.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByText("demo")).toBeNull();
  fireEvent.click(button);
  expect(screen.getByRole("button", { name: "Collapse loom plan Engineering Metaplan" })).toBe(
    button,
  );
  expect(button.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByText("demo")).toBeTruthy();
});

it("encodes the gap title, feature and target in the tree link", () => {
  renderEntry({
    gap: true,
    path: null,
    title: "Map & search",
    feature: "beta +/雪",
    target: "doc/product/beta +/雪/section_03.md",
  });
  expect(screen.getByRole("link").getAttribute("href")).toBe(
    "/files?gap=Map%20%26%20search&feature=beta%20%2B%2F%E9%9B%AA&target=doc%2Fproduct%2Fbeta%20%2B%2F%E9%9B%AA%2Fsection_03.md",
  );
});

it("omits the feature from a standalone gap link", () => {
  renderEntry({ gap: true, path: null, title: "Search", feature: null });
  expect(screen.getByRole("link").getAttribute("href")).toBe("/files?gap=Search");
});

function gapChecklist(feature: string) {
  const checklist = checklistViewSchema.parse(structuredClone(checklistFixture.checklists[0]));
  return {
    ...checklist,
    feature,
    items: ["Sitemap", "Search", "Map view"].map((title, index) => ({
      step: 2,
      text: `section 0${index + 1}: ${title}`,
      path: `doc/product/${feature}/section_0${index + 1}.md`,
      offer: {
        id: `write:${feature}:${title}`,
        kind: "write_section" as const,
        label: `Write ${feature} ${title}`,
        target: `doc/product/${feature}/section_0${index + 1}.md`,
        feature,
        number: index + 1,
      },
    })),
  };
}

function renderGapMode(entry = "/files", feature = "demo") {
  vi.stubGlobal("matchMedia", () => ({
    matches: true,
    addEventListener() {},
    removeEventListener() {},
  }));
  const store = createStore();
  const checklists = ["alpha", feature].map(gapChecklist);
  store.set(projectAtom, { checklists, standalone: [], moves: [] });
  store.set(activeFeatureAtom, "alpha");
  const tree = fileTreeSchema.parse(structuredClone(treeFixture));
  tree.product = checklists.map(({ feature }) => ({
    ...tree.product[0]!,
    feature,
    title: feature,
    path: null,
    gap: false,
    children: [
      {
        ...tree.product[0]!,
        feature,
        title: "Map view",
        kind: "section",
        path: null,
        target: `doc/product/${feature}/section_03.md`,
        gap: true,
        children: [],
      },
    ],
  }));
  store.set(treeAtom, tree);
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[entry]}>
        <FileMode />
      </MemoryRouter>
    </Provider>,
  );
}

it("clicking demo's Map view gap uses its checklist and offer when alpha has the same title", () => {
  const handler = vi.fn();
  const unregister = registerOfferAction("write_section", handler);
  try {
    renderGapMode();
    const navigation = within(screen.getByRole("navigation", { name: "Documents" }));
    fireEvent.click(navigation.getByRole("button", { name: "Product" }));
    fireEvent.click(navigation.getAllByRole("link", { name: "Map view" })[1]!);
    const document = within(screen.getByRole("region", { name: "Document" }));
    expect(document.getByRole("heading", { name: "Map view" })).toBeTruthy();
    expect(document.getByText("section 03: Map view")).toBeTruthy();
    expect(document.queryByText("Write alpha Map view")).toBeNull();
    fireEvent.click(document.getByRole("button", { name: "Write demo Map view" }));
    expect(handler).toHaveBeenCalledOnce();
    expect(handler).toHaveBeenCalledWith(
      expect.objectContaining({ feature: "demo", id: "write:demo:Map view" }),
    );
  } finally {
    unregister();
  }
});

it("reloading a gap matches its target even when the title differs from the checklist label", () => {
  renderGapMode("/files?gap=Map%20view&feature=demo&target=doc%2Fproduct%2Fdemo%2Fsection_03.md");
  const document = within(screen.getByRole("region", { name: "Document" }));
  expect(document.getByText("Step 2")).toBeTruthy();
  expect(document.getByText("Write demo Map view")).toBeTruthy();
  expect(document.queryByText("Write demo Sitemap")).toBeNull();
});

it.each([
  "/files?gap=Map%20view&target=doc%2Fproduct%2Fdemo%2Fsection_03.md",
  "/files?gap=Map%20view&feature=unknown&target=doc%2Fproduct%2Fdemo%2Fsection_03.md",
  "/files?gap=Map%20view&feature=alpha&target=doc%2Fproduct%2Fdemo%2Fsection_03.md",
  "/files?gap=Map%20view&feature=demo&target=doc%2Fproduct%2Fdemo%2Fsection_30.md",
  "/files?gap=section%2003%3A%20Map%20view&feature=demo",
])("shows no checklist item for an unowned, unknown or absent target at %s", (entry) => {
  renderGapMode(entry);
  const document = within(screen.getByRole("region", { name: "Document" }));
  expect(document.getByText("No checklist item asks for it yet.")).toBeTruthy();
  expect(document.queryByText(/^Write /)).toBeNull();
});

it("resolves an encoded feature from the gap link", () => {
  renderGapMode("/files", "beta +/雪");
  const navigation = within(screen.getByRole("navigation", { name: "Documents" }));
  fireEvent.click(navigation.getByRole("button", { name: "Product" }));
  fireEvent.click(navigation.getAllByRole("link", { name: "Map view" })[1]!);
  const document = within(screen.getByRole("region", { name: "Document" }));
  expect(document.getByText("Write beta +/雪 Map view")).toBeTruthy();
});
