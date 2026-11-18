import { fireEvent, render, screen, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { panelLoadAtom, summaryReloadAtom, SummaryFeedHost } from "@/plan/summaries/state";
import { SettingsDialog } from "./settings-dialog";
import { openPane, useSettingsTestLifecycle } from "./settings-dialog-kit";

useSettingsTestLifecycle();

it("Settings shows the summary totals", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(panelFixture), { status: 200 })),
  );
  const store = createStore();
  render(
    <Provider store={store}>
      <TooltipProvider>
        <SummaryFeedHost />
        <SettingsDialog open onOpenChange={() => {}} />
      </TooltipProvider>
    </Provider>,
  );
  openPane("Summaries");
  const dialog = screen.getByRole("dialog", { name: "Settings" });
  expect(within(dialog).getByRole("heading", { name: "Summaries" })).toBeTruthy();

  const totals = await within(dialog).findByRole("region", { name: "Token totals" });
  expect(within(totals).getByText("12,400")).toBeTruthy();
  expect(within(dialog).getByText("Rebuild token totals")).toBeTruthy();
});

it("Settings keeps the heading outline: the panel's h3 sections sit under an h2 Summaries", async () => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(panelFixture), { status: 200 })),
  );
  render(
    <Provider store={createStore()}>
      <TooltipProvider>
        <SummaryFeedHost />
        <SettingsDialog open onOpenChange={() => {}} />
      </TooltipProvider>
    </Provider>,
  );
  openPane("Summaries");
  const dialog = screen.getByRole("dialog", { name: "Settings" });
  expect(within(dialog).getByRole("heading", { name: "Summaries", level: 2 })).toBeTruthy();
  const totals = await within(dialog).findByRole("region", { name: "Token totals" });
  expect(within(totals).getByRole("heading", { level: 3 })).toBeTruthy();
});

it("Settings offers a retry when the summary feed failed", async () => {
  const reload = vi.fn();
  const store = createStore();
  store.set(panelLoadAtom, { status: "error", message: "HTTP 500" });
  store.set(summaryReloadAtom, { reload });
  render(
    <Provider store={store}>
      <SettingsDialog open onOpenChange={() => {}} />
    </Provider>,
  );

  openPane("Summaries");
  const dialog = screen.getByRole("dialog", { name: "Settings" });
  fireEvent.click(within(dialog).getByRole("button", { name: "Retry" }));

  expect(reload).toHaveBeenCalledOnce();
});
