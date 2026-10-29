import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import { Tabs } from "@/plan/header/tabs";

afterEach(cleanup);

describe("Tabs", () => {
  it("keeps Home available without registered tabs", () => {
    render(
      <MemoryRouter>
        <Tabs tabs={[]} />
      </MemoryRouter>,
    );
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
    render(
      <MemoryRouter initialEntries={["/files"]}>
        <Tabs
          tabs={[
            { id: "files", label: "Files", path: "/files", element },
            { id: "product", label: "Product", path: "/product", element },
          ]}
        />
      </MemoryRouter>,
    );
    // jsdom does not apply responsive utilities, so both label copies contribute to names.
    const files = screen.getByRole("link", { name: /Files/ });
    const product = screen.getByRole("link", { name: /Product/ });
    const home = screen.getByRole("link", { name: /Home/ });
    expect(files.getAttribute("href")).toBe("/files");
    expect(product.getAttribute("href")).toBe("/product");
    expect(files.getAttribute("aria-current")).toBe("page");
    expect(product.getAttribute("aria-current")).toBeNull();
    expect(home.getAttribute("aria-current")).toBeNull();
    expect(files.className).toBe("view-tab is-active");
    expect(product.className).toBe("view-tab");
    expect(home.className).toBe("view-tab");
  });

  it("renders the default views with icons and accessible labels on narrow screens", () => {
    render(
      <MemoryRouter>
        <Tabs />
      </MemoryRouter>,
    );
    expect(screen.getByRole("navigation", { name: "Views" }).className).toBe("view-switch");
    const links = screen.getAllByRole("link");
    expect(links.map((link) => link.getAttribute("href"))).toEqual([
      "/",
      "/product",
      "/engineering",
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
    render(
      <MemoryRouter initialEntries={["/files/docs/intro.md"]}>
        <Tabs />
      </MemoryRouter>,
    );
    expect(screen.getByRole("link", { name: /Files/ }).className).toBe("view-tab is-active");
    expect(screen.getByRole("link", { name: /Home/ }).className).toBe("view-tab");
  });

  it.each(["custom", "constructor", "toString", "__proto__"])(
    "renders an unrecognized tab id %s without an icon",
    (id) => {
      render(
        <MemoryRouter initialEntries={["/custom"]}>
          <Tabs tabs={[{ id, label: "Custom", path: "/custom" }]} />
        </MemoryRouter>,
      );
      const custom = screen.getByRole("link", { name: /Custom/ });
      expect(custom.getAttribute("href")).toBe("/custom");
      expect(custom.getAttribute("aria-current")).toBe("page");
      expect(custom.className).toBe("view-tab is-active");
      expect(custom.querySelector("svg")).toBeNull();
    },
  );
});
