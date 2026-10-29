import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import { Tabs } from "@/plan/header/tabs";

afterEach(cleanup);

describe("Tabs", () => {
  it("renders nothing without registered tabs", () => {
    const { container } = render(
      <MemoryRouter>
        <Tabs tabs={[]} />
      </MemoryRouter>,
    );
    expect(container.innerHTML).toBe("");
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
    expect(screen.getByRole("link", { name: "Files" }).getAttribute("aria-current")).toBe("page");
    expect(screen.getByRole("link", { name: "Product" }).getAttribute("aria-current")).toBeNull();
  });

  it("shrinks and scrolls instead of pushing the header controls out of a narrow screen", () => {
    const element = () => <p />;
    render(
      <MemoryRouter>
        <Tabs tabs={[{ id: "files", label: "Files", path: "/files", element }]} />
      </MemoryRouter>,
    );
    const nav = screen.getByRole("navigation", { name: "Planning tabs" });
    expect(nav.className).toContain("min-w-0");
    expect(nav.className).toContain("overflow-x-auto");
    expect(screen.getByRole("link", { name: "Files" }).className).toContain("shrink-0");
  });
});
