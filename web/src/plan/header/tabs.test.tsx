import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import { Tabs } from "@/plan/header/tabs";
import type { PlanTab } from "@/plan/tabs";

afterEach(cleanup);

function show({ pathname = "/", tabs }: { pathname?: string; tabs?: PlanTab[] } = {}) {
  render(
    <MemoryRouter initialEntries={[pathname]}>
      <Tabs tabs={tabs} />
    </MemoryRouter>,
  );
}

describe("Tabs", () => {
  it("keeps Home available without registered tabs", () => {
    show({ tabs: [] });
    const nav = screen.getByRole("navigation", { name: "Views" });
    expect(nav.className).toBe("view-switch plan-view-switch");
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
      tabs: [
        { id: "files", label: "Files", path: "/files", element },
        { id: "notes", label: "Notes", path: "/notes" },
      ],
    });
    const files = screen.getByRole("link", { name: /Files/ });
    const notes = screen.getByRole("link", { name: /Notes/ });
    const home = screen.getByRole("link", { name: /Home/ });
    expect(files.getAttribute("href")).toBe("/files");
    expect(notes.getAttribute("href")).toBe("/notes");
    expect(files.getAttribute("aria-current")).toBe("page");
    expect(notes.getAttribute("aria-current")).toBeNull();
    expect(home.getAttribute("aria-current")).toBeNull();
    expect(files.className).toBe("view-tab is-active");
    expect(notes.className).toBe("view-tab");
    expect(home.className).toBe("view-tab");
  });

  it("renders Home and Files with icons and accessible labels on narrow screens", () => {
    show();
    expect(screen.getByRole("navigation", { name: "Views" }).className).toBe(
      "view-switch plan-view-switch",
    );
    const links = screen.getAllByRole("link");
    expect(links.map((link) => link.getAttribute("href"))).toEqual(["/", "/files"]);
    const glyphs = ["lucide-house", "lucide-folder-tree"];
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

  it.each(["/f/demo/product", "/f/demo/engineering", "/plans/standalone"])(
    "marks no view active on the feature page %s",
    (pathname) => {
      show({ pathname });
      for (const link of screen.getAllByRole("link")) {
        expect(link.getAttribute("aria-current")).toBeNull();
      }
      expect(screen.queryByRole("link", { name: /Product|Engineering/ })).toBeNull();
    },
  );

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
