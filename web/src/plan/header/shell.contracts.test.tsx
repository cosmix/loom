import { cleanup, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import runStripFixture from "@/plan/api/fixtures/run-strip.json";
import { runStripSchema } from "@/plan/api/schema-run";
import { Dock } from "@/plan/dock";
import { PlanHeader } from "@/plan/header/header";
import { StepTracker } from "@/plan/header/step-tracker";
import { runStripAtom } from "@/plan/run/use-run";

import { demoChecklist, demoStore, desktopMedia, renderAt } from "./shell-contracts-kit";

// The panel's desktop region, without its socket and transcript; the dock decides where it mounts.
vi.mock("@/plan/chat/panel", async (original) => ({
  ...(await original<typeof import("@/plan/chat/panel")>()),
  ChatPanel: () => <aside aria-label="Chat" />,
  ChatRegistrations: () => null,
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("planner shell", () => {
  it("home route renders no chat panel", () => {
    desktopMedia();
    renderAt("/f/demo/product", <Dock />, demoStore());
    expect(screen.getByRole("complementary", { name: "Chat" })).toBeTruthy();
    cleanup();

    renderAt("/", <Dock />, demoStore());
    expect(screen.queryByRole("complementary", { name: "Chat" })).toBeNull();
  });

  it("header shows no run marquee", () => {
    desktopMedia();
    const store = demoStore();
    store.set(runStripAtom, {
      ...runStripSchema.parse(runStripFixture),
      plan: "doc/plans/DONE-PLAN-x.md",
      daemon_running: false,
      merged: 0,
      total: 10,
    });
    renderAt("/f/demo", <PlanHeader onOpenSettings={() => {}} onOpenHelp={() => {}} />, store);

    const banner = screen.getByRole("banner");
    expect(banner.textContent).not.toContain("daemon stopped");
  });

  it("rail cell shows the server count and links to its step", () => {
    const steps = demoChecklist().steps.map((step) =>
      step.number === 2 ? { ...step, count: "12/12", label: "Product spec" } : step,
    );
    renderAt("/f/demo", <StepTracker checklist={demoChecklist({ steps })} narrow={false} />);

    const cell = screen.getByText("Product spec").closest("li")!;
    expect(within(cell).getByText("12/12")).toBeTruthy();
    expect(within(cell).queryByText("2")).toBeNull();
    expect(within(cell).getByRole("link").getAttribute("href")).toBe("/f/demo?step=2");
  });
});
