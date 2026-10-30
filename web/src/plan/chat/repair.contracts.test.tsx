import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import {
  TRANSCRIPT_TEXT,
  installRepairLifecycle,
  mountPanel,
  mountPressure,
  waitingSessionFrame,
} from "./repair-contracts-kit";

const PERMISSION = "Permission: Bash";
const WRITTEN_PLAN = "doc/plans/PLAN-example.md";
const TITLED_PLAN = "doc/plans/PLAN-adopt-ecc-best-practices.md";
const PLAN_TITLE = "Plan: Adopt Best Practices from Everything Claude Code";

installRepairLifecycle();

describe("chat repair", () => {
  it("permission card renders once", async () => {
    const panel = await mountPanel("/f/demo/product", waitingSessionFrame("demo"));

    const cards = await within(panel).findAllByRole("region", { name: PERMISSION });

    expect(cards).toHaveLength(1);
  });

  it("a session from another feature is not shown", async () => {
    const panel = await mountPanel("/f/beta/product", waitingSessionFrame("alpha"));

    const notice = await within(panel).findByRole("link", {
      name: "A session in Alpha needs you",
    });

    expect(notice.getAttribute("href")).toMatch(/^\/f\/alpha(?:[/?]|$)/);
    expect(within(panel).queryByText(TRANSCRIPT_TEXT)).toBeNull();
    expect(within(panel).queryByRole("region", { name: PERMISSION })).toBeNull();
  });

  it("pressure dialog offers quick pick rounds only", async () => {
    const { fetchSpy } = await mountPressure(WRITTEN_PLAN, "Example plan");

    expect(screen.queryAllByRole("spinbutton")).toEqual([]);
    expect(screen.getByRole("button", { name: "1" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "2" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "3" }));
    fireEvent.click(screen.getByRole("button", { name: "Start pressure test" }));

    const started = () =>
      fetchSpy.mock.calls.filter(([input]) => String(input).includes("/api/pressure/start"));
    await waitFor(() => expect(started()).toHaveLength(1));
    expect(JSON.parse(String(started()[0]?.[1]?.body)).rounds).toBe(3);
  });

  it("pressure plan card shows the plan title", async () => {
    await mountPressure(TITLED_PLAN, PLAN_TITLE);

    expect(await screen.findByRole("heading", { name: PLAN_TITLE })).toBeTruthy();
    expect(screen.queryByText(/adopt ecc best practices/i)).toBeNull();
  });
});
