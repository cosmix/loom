import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it, vi } from "vitest";

import treeFixture from "@/plan/api/fixtures/files-tree.json";
import {
  fileTreeSchema,
  type DocView,
  type FileTree,
  type TreeEntry,
} from "@/plan/api/schema-files";
import { openDocAtom } from "@/plan/files/atoms";
import { Tree } from "@/plan/files/tree";
import { openQuestionsAtom } from "@/plan/review/live";

vi.mock("@/plan/files/api", () => ({ searchDocs: vi.fn() }));

type Store = ReturnType<typeof createStore>;

const base = (): TreeEntry => fileTreeSchema.parse(structuredClone(treeFixture)).plans[0]!;

function plan(n: string, title: string, state: TreeEntry["state"], extra: Partial<TreeEntry> = {}) {
  const file = `PLAN-feat-${n}.md`;
  return {
    ...base(),
    feature: "feat",
    title,
    state,
    pressure_date: null,
    errors: 0,
    file_name: file,
    path: `doc/plans/feat/${file}`,
    ...extra,
  };
}

const folder = (name: string): TreeEntry => ({
  ...base(),
  kind: "brief_folder",
  feature: "feat",
  title: name,
  state: null,
  path: null,
  file_name: null,
  children: [],
});

const METAPLAN = "doc/plans/feat/index.md";

function featureTree(): FileTree {
  const tree = fileTreeSchema.parse(structuredClone(treeFixture));
  const stages = [folder("checklist-check"), folder("document-model")];
  const gap: TreeEntry = {
    ...folder("x"),
    kind: "metaplan_entry",
    title: "Later work",
    gap: true,
    target: "doc/plans/feat/PLAN-feat-04-later.md",
  };
  const metaplan: TreeEntry = {
    ...base(),
    kind: "metaplan",
    feature: "feat",
    state: null,
    title: "`loom plan` Engineering Metaplan",
    file_name: "index.md",
    path: METAPLAN,
    children: [
      plan("01-model", "Plan: Document model", "done", { children: stages }),
      plan("02-server", "Plan: Server core", "done"),
      plan("03-web", "Plan: Web shell", "written"),
      gap,
    ],
  };
  tree.plans = [{ ...folder("feat"), kind: "feature", children: [metaplan] }];
  return tree;
}

function renderTree(
  tree: FileTree,
  setup: (store: Store) => void = () => {},
  open: string | null = null,
) {
  const store = createStore();
  setup(store);
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={["/files"]}>
        <Tree tree={tree} open={open} />
      </MemoryRouter>
    </Provider>,
  );
  return store;
}

const row = (name: RegExp) => screen.getByRole("button", { name });
const entry = (name: RegExp) => row(name).closest("li") as HTMLElement;

afterEach(cleanup);

describe("the plans rail", () => {
  it("opens on Plans: the feature's head, its count, and one closed row per plan", () => {
    renderTree(featureTree());

    expect(screen.getByRole("button", { name: "Plans" }).getAttribute("aria-pressed")).toBe("true");
    const head = screen.getByRole("link", { name: "feat" });
    expect(head.getAttribute("href")).toBe("/files/plans/feat/index.md");
    expect(head.getAttribute("title")).toBe("Metaplan: loom plan Engineering Metaplan");
    expect(screen.getByText("2 of 4 plans complete")).toBeTruthy();
    const first = row(/^01 Document model/);
    expect(first.getAttribute("aria-expanded")).toBe("false");
    expect(within(first).getByText("checklist-check · document-model")).toBeTruthy();
    expect(first.querySelector('[data-node="done"]')).not.toBeNull();
    expect(row(/^03 Web shell/).querySelector('[data-node="written"]')).not.toBeNull();
    // Done is the node alone: no visible word, lock or dot on the row.
    expect(first.querySelector(".lucide-lock")).toBeNull();
    expect(within(first).getByText(", done").classList.contains("sr-only")).toBe(true);
  });

  it("opens a plan's documents on click: the plan first, then its stages", () => {
    renderTree(featureTree());
    fireEvent.click(row(/^01 Document model/));

    const opened = entry(/^01 Document model/);
    expect(row(/^01 Document model/).getAttribute("aria-expanded")).toBe("true");
    expect(within(opened).getByRole("link", { name: "Plan document" }).getAttribute("href")).toBe(
      "/files/plans/feat/PLAN-feat-01-model.md",
    );
    expect(within(opened).getByText("checklist-check").closest(".plan-tree__row")).not.toBeNull();
  });

  it("starts a plan open when it is running or has errors, closed when it is settled", () => {
    const tree = featureTree();
    const plans = tree.plans[0]!.children[0]!.children;
    plans[1] = { ...plans[1]!, state: "running" };
    plans[2] = { ...plans[2]!, errors: 2 };
    renderTree(tree);

    expect(row(/^02 Server core/).getAttribute("aria-expanded")).toBe("true");
    const broken = row(/^03 Web shell/);
    expect(broken.getAttribute("aria-expanded")).toBe("true");
    expect(broken.querySelector('[data-node="error"]')).not.toBeNull();
    expect(within(broken).getByText(", 2 errors")).toBeTruthy();
    expect(row(/^01 Document model/).getAttribute("aria-expanded")).toBe("false");
  });

  it("draws one question bar linking to the document's open questions", () => {
    renderTree(featureTree(), (store) =>
      store.set(openQuestionsAtom, {
        [METAPLAN]: 1,
        "doc/plans/feat/PLAN-feat-03-web.md": 2,
      }),
    );

    const bars = screen.getAllByRole("link", { name: /open questions?$/ });
    expect(bars).toHaveLength(1);
    expect(bars[0]!.textContent).toBe("Metaplan and 1 more 3 open questions");
    expect(bars[0]!.getAttribute("href")).toBe("/files/plans/feat/index.md#open-questions");
    // A plan with questions asks for the reader: its node says so and it starts open.
    const asking = row(/^03 Web shell/);
    expect(asking.querySelector('[data-node="question"]')).not.toBeNull();
    expect(asking.getAttribute("aria-expanded")).toBe("true");
  });

  it("opens and marks the plan holding the open document, a finished one included", () => {
    const doc = { path: "doc/plans/feat/PLAN-feat-02-server.md", sections: [] };
    renderTree(featureTree(), (store) =>
      store.set(openDocAtom, { doc: doc as unknown as DocView, seenHash: "" }),
    );

    const holder = entry(/^02 Server core/);
    expect(holder.hasAttribute("data-current")).toBe(true);
    const link = within(holder).getByRole("link", { name: "Plan document" });
    expect(link.getAttribute("aria-current")).toBe("page");
  });

  it("links a plan not written yet to its gap page", () => {
    renderTree(featureTree());

    const gap = screen.getByRole("link", { name: /^Later work/ });
    expect(gap.getAttribute("href")).toContain("/files?gap=Later%20work&feature=feat");
    expect(gap.querySelector('[data-node="gap"]')).not.toBeNull();
    expect(within(gap).getAllByText(/not written/).length).toBeGreaterThan(0);
  });

  it("lists the product spec on the Product tab, the vision first as Vision", () => {
    renderTree(featureTree());
    fireEvent.click(screen.getByRole("button", { name: "Product" }));

    const product = screen.getByRole("region", { name: "demo" });
    const links = within(product).getAllByRole("link");
    expect(links[0]!.textContent).toBe("Vision");
    expect(links[0]!.getAttribute("href")).toBe("/files/product/demo/index.md");
    expect(within(product).getByText("1 section · 1 missing")).toBeTruthy();
    expect(screen.queryByText("Web shell")).toBeNull();
  });

  it("chooses the tab that lists the open document", () => {
    renderTree(featureTree(), () => {}, "doc/product/demo/index.md");
    expect(screen.getByRole("button", { name: "Product" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
  });

  it("drops the rail while filtering, folding finished plans that match", () => {
    renderTree(featureTree());
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "server" } });

    expect(screen.queryByRole("button", { name: "Plans", pressed: true })).toBeNull();
    expect(screen.getByRole("link", { name: "02 Server core" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "03 Web shell" })).toBeNull();
  });
});
