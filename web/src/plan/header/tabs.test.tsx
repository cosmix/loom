import { cleanup, render, screen } from "@testing-library/react";
import { Provider } from "jotai/react";
import { createStore } from "jotai/vanilla";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import checklistFixture from "@/plan/api/fixtures/checklist.json";
import { checklistViewSchema } from "@/plan/api/schema";
import { Tabs } from "@/plan/header/tabs";
import { activeFeatureAtom, projectAtom } from "@/plan/state/atoms";
import type { PlanTab } from "@/plan/tabs";

afterEach(cleanup);

function show({
  pathname = "/",
  features = [],
  activeFeature = null,
  tabs,
}: {
  pathname?: string;
  features?: string[];
  activeFeature?: string | null;
  tabs?: PlanTab[];
} = {}) {
  const store = createStore();
  const checklist = checklistViewSchema.parse(checklistFixture.checklists[0]);
  store.set(activeFeatureAtom, activeFeature);
  store.set(projectAtom, {
    checklists: features.map((feature) => ({ ...checklist, feature })),
    standalone: [],
    moves: [],
  });
  render(
    <Provider store={store}>
      <MemoryRouter initialEntries={[pathname]}>
        <Tabs tabs={tabs} />
      </MemoryRouter>
    </Provider>,
  );
}

describe("Tabs", () => {
  it("keeps Home available without registered tabs", () => {
    show({ tabs: [] });
    const nav = screen.getByRole("navigation", { name: "Views" });
    expect(nav.className).toBe("view-switch");
    expect(screen.getAllByRole("link")).toHaveLength(1);
    const home = screen.getByRole("link", { name: /Home/ });
    expect(home.getAttribute("href")).toBe("/");
    expect(home.getAttribute("aria-current")).toBe("page");
    expect(home.classList.contains("is-active")).toBe(true);
  });

  it("links each registered tab and marks the current route", () => {
    const element = () => <p />;
    show({
      pathname: "/files",
      features: ["demo"],
      tabs: [
        { id: "files", label: "Files", path: "/files", element },
        { id: "product", label: "Product", path: null },
      ],
    });
    const files = screen.getByRole("link", { name: /Files/ });
    const product = screen.getByRole("link", { name: /Product/ });
    const home = screen.getByRole("link", { name: /Home/ });
    expect(files.getAttribute("href")).toBe("/files");
    expect(product.getAttribute("href")).toBe("/f/demo/product");
    expect(files.getAttribute("aria-current")).toBe("page");
    expect(product.getAttribute("aria-current")).toBeNull();
    expect(home.getAttribute("aria-current")).toBeNull();
    expect(files.className).toBe("view-tab is-active");
    expect(product.className).toBe("view-tab");
    expect(home.className).toBe("view-tab");
  });

  it("renders the default views with icons and accessible labels on narrow screens", () => {
    show({ features: ["demo"] });
    expect(screen.getByRole("navigation", { name: "Views" }).className).toBe("view-switch");
    const links = screen.getAllByRole("link");
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      "/",
      "/f/demo/product",
      "/f/demo/engineering",
      "/files",
    ]);
    const glyphs = ["lucide-house", "lucide-package", "lucide-cog", "lucide-folder-tree"];
    for (const [index, link] of links.entries()) {
      const icon = link.querySelector("svg");
      expect(icon?.classList.contains(glyphs[index]!)).toBe(true);
      expect(icon?.classList.contains("size-3.5")).toBe(true);
      expect(icon?.getAttribute("aria-hidden")).toBe("true");
      const labels = link.querySelectorAll("span");
      expect(labels).toHaveLength(1);
      expect(labels[0]?.className).toBe("max-sm:sr-only");
    }
  });

  it("keeps a view active on nested routes while Home stays inactive", () => {
    show({ pathname: "/files/docs/intro.md" });
    expect(screen.getByRole("link", { name: /Files/ }).className).toBe("view-tab is-active");
    expect(screen.getByRole("link", { name: /Home/ }).className).toBe("view-tab");
  });

  it("uses the active feature among several from Files", () => {
    show({ pathname: "/files", features: ["first", "demo"], activeFeature: "demo" });
    expect(screen.getByRole("link", { name: /Product/ }).getAttribute("href")).toBe(
      "/f/demo/product",
    );
    expect(screen.getByRole("link", { name: /Engineering/ }).getAttribute("href")).toBe(
      "/f/demo/engineering",
    );
  });

  it.each(["/f/current", "/f/current/product/vision", "/f/current/engineering/plan"])(
    "prefers the feature in %s over the remembered feature",
    (pathname) => {
      show({ pathname, features: ["remembered", "current"], activeFeature: "remembered" });
      expect(screen.getByRole("link", { name: /Product/ }).getAttribute("href")).toBe(
        "/f/current/product",
      );
      expect(screen.getByRole("link", { name: /Engineering/ }).getAttribute("href")).toBe(
        "/f/current/engineering",
      );
    },
  );

  it.each([{ features: [] }, { features: ["first", "second"] }])(
    "disables concept views without a selected or sole feature: $features",
    ({ features }) => {
      show({ features });
      for (const name of [/Product/, /Engineering/]) {
        const tab = screen.getByRole("link", { name });
        expect(tab.getAttribute("aria-disabled")).toBe("true");
        expect(tab.getAttribute("href")).toBeNull();
        expect(tab.tabIndex).toBe(-1);
        expect(tab.title).toBe("Open a feature first");
      }
      expect(screen.getByRole("link", { name: /Home/ }).getAttribute("href")).toBe("/");
      expect(screen.getByRole("link", { name: /Files/ }).getAttribute("href")).toBe("/files");
    },
  );

  it.each([
    ["/f/demo/product/vision", "Product"],
    ["/f/demo/engineering/implementation", "Engineering"],
    ["/plans/standalone", "Engineering"],
  ])("marks the view active on summary path %s", (pathname, name) => {
    show({ pathname, features: ["demo"] });
    const tab = screen.getByRole("link", { name });
    expect(tab.getAttribute("aria-current")).toBe("page");
    expect(tab.className).toBe("view-tab is-active");
    expect(screen.getByRole("link", { name: /Home/ }).getAttribute("aria-current")).toBeNull();
    const other = name === "Product" ? /Engineering/ : /Product/;
    expect(screen.getByRole("link", { name: other }).getAttribute("aria-current")).toBeNull();
  });

  it("marks standalone Engineering active even when its overview target is disabled", () => {
    show({ pathname: "/plans/standalone" });
    const engineering = screen.getByRole("link", { name: /Engineering/ });
    expect(engineering.getAttribute("aria-current")).toBe("page");
    expect(engineering.getAttribute("aria-disabled")).toBe("true");
    expect(engineering.getAttribute("href")).toBeNull();
  });

  it.each(["custom", "constructor", "toString", "__proto__"])(
    "renders an unrecognized tab id %s without an icon",
    (id) => {
      show({ pathname: "/custom", tabs: [{ id, label: "Custom", path: "/custom" }] });
      const custom = screen.getByRole("link", { name: /Custom/ });
      expect(custom.getAttribute("href")).toBe("/custom");
      expect(custom.getAttribute("aria-current")).toBe("page");
      expect(custom.className).toBe("view-tab is-active");
      expect(custom.querySelector("svg")).toBeNull();
    },
  );
});
