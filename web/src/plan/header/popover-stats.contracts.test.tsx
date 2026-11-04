import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { getDefaultStore } from "jotai/vanilla";
import { afterEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";
import { ActivityPill } from "@/plan/header/activity";
import { useActivities } from "@/plan/header/use-activities";
import { panelLoadAtom, useSummaryFeed, type PanelLoad } from "@/plan/summaries/state";

vi.mock("@/plan/run/dashboard-button", () => ({
  DashboardButton: () => <a href="/dashboard">Open dashboard</a>,
}));
vi.mock("@/plan/summaries/state", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/plan/summaries/state")>()),
  useSummaryFeed: vi.fn(),
}));
vi.mock("@/plan/header/use-activities", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/plan/header/use-activities")>()),
  useActivities: vi.fn(),
}));

afterEach(() => {
  cleanup();
  getDefaultStore().set(panelLoadAtom, { status: "loading" });
  vi.clearAllMocks();
});

function setPanel(): void {
  const load: PanelLoad = { status: "ready", panel: summaryPanelSchema.parse(panelFixture) };
  getDefaultStore().set(panelLoadAtom, load);
  vi.mocked(useSummaryFeed).mockReturnValue({ load, reload: vi.fn() });
}

async function openIdle(): Promise<HTMLElement> {
  vi.mocked(useActivities).mockReturnValue([]);
  render(
    <TooltipProvider>
      <ActivityPill compact={false} />
    </TooltipProvider>,
  );
  const opened = screen.queryByRole("dialog", { name: "In progress" });
  if (opened) return opened;
  fireEvent.click(screen.getByRole("button", { name: /^Activity:/ }));
  return screen.findByRole("dialog", { name: "In progress" });
}

function expand(dialog: HTMLElement): void {
  const row = within(dialog).getByRole("button", { name: /^Summary rebuilds:/ });
  if (row.getAttribute("aria-expanded") !== "true") fireEvent.click(row);
}

it("the activity popover shows no limits and no token totals", async () => {
  setPanel();
  const dialog = await openIdle();
  expand(dialog);

  expect(within(dialog).getByRole("button", { name: /^Summary rebuilds:/ })).toBeTruthy();
  expect(within(dialog).queryByRole("region", { name: "Token totals" })).toBeNull();
  expect(within(dialog).queryAllByRole("img", { name: /window .*% used/ })).toHaveLength(0);
});
