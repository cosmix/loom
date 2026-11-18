import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { SummaryFeedHost } from "@/plan/summaries/state";
import { SettingsDialog } from "./settings-dialog";

beforeEach(() => {
  localStorage.clear();
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(panelFixture), { status: 200 })),
  );
});

afterEach(() => {
  cleanup();
  localStorage.clear();
  vi.unstubAllGlobals();
});

it("Settings shows the summary statistics and no provider limits", async () => {
  render(
    <Provider store={createStore()}>
      <TooltipProvider>
        <SummaryFeedHost />
        <SettingsDialog open onOpenChange={() => {}} />
      </TooltipProvider>
    </Provider>,
  );
  fireEvent.click(screen.getByRole("tab", { name: "Summaries" }));
  const dialog = screen.getByRole("dialog", { name: "Settings" });
  expect(await within(dialog).findByRole("region", { name: "Token totals" })).toBeTruthy();
  expect(within(dialog).queryAllByRole("img", { name: /window .*% used/ })).toHaveLength(0);
});
