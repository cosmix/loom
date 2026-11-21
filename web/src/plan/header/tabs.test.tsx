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
  it("omits the navigation when there are no registered views", () => {
    show({ tabs: [] });
    expect(screen.queryByRole("navigation", { name: "Views" })).toBeNull();
  });

  it("links each registered tab and marks the current route", () => {
    const element = () => <p />;
    show({
      pathname: "/files",
      tabs: [
        { id: "files", label: "Files", path: "/files", element },
        { id: "notes", label: "Notes", path: "/notes", element },
      ],
    });
    const files = screen.getByRole("link", { name: /Files/ });
    const notes = screen.getByRole("link", { name: /Notes/ });
    expect(files.getAttribute("href")).toBe("/files");
    expect(notes.getAttribute("href")).toBe("/notes");
    expect(files.getAttribute("aria-current")).toBe("page");
    expect(notes.getAttribute("aria-current")).toBeNull();
    expect(files.className).toBe("view-tab is-active");
    expect(notes.className).toBe("view-tab");
  });

  it("keeps Files labeled and active on nested file routes", () => {
    show({ pathname: "/files/docs/intro.md" });
    const files = screen.getByRole("link", { name: "Files" });
    expect(files.getAttribute("aria-current")).toBe("page");
    expect(files.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
    expect(screen.queryByRole("link", { name: "Home" })).toBeNull();
  });

  it.each(["/f/demo/product", "/f/demo/metaplan", "/plans/standalone"])(
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
      show({
        pathname: "/custom",
        tabs: [{ id, label: "Custom", path: "/custom", element: () => <p /> }],
      });
      const custom = screen.getByRole("link", { name: /Custom/ });
      expect(custom.getAttribute("href")).toBe("/custom");
      expect(custom.getAttribute("aria-current")).toBe("page");
      expect(custom.className).toBe("view-tab is-active");
      expect(custom.querySelector("svg")).toBeNull();
    },
  );
});
