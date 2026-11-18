import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore, Provider } from "jotai";
import { useState } from "react";
import { describe, expect, it } from "vitest";

import { SettingsDialog } from "./settings-dialog";
import { renderSettings, useSettingsTestLifecycle } from "./settings-dialog-kit";

function SettingsLauncher() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Open settings
      </button>
      <SettingsDialog open={open} onOpenChange={setOpen} />
    </>
  );
}

useSettingsTestLifecycle();

describe("planner settings sections", () => {
  it("renders the title, a hidden browser description and a rail of five tabs on Appearance", () => {
    renderSettings();

    const dialog = screen.getByRole("dialog", { name: "Settings" });
    const description = "Saved in this browser; web fetches are saved for this project";
    expect(within(dialog).getByText(description).className).toContain("sr-only");
    const tabs = within(screen.getByRole("tablist", { name: "Settings sections" })).getAllByRole(
      "tab",
    );
    expect(tabs.map((tab) => tab.textContent)).toEqual([
      "Appearance",
      "Reading",
      "Notifications",
      "Web fetches",
      "Summaries",
    ]);
    expect(tabs.map((tab) => tab.getAttribute("aria-selected"))).toEqual([
      "true",
      "false",
      "false",
      "false",
      "false",
    ]);
    expect(within(dialog).getByRole("heading", { name: "Appearance", level: 2 })).toBeTruthy();
    expect(within(dialog).getByRole("heading", { name: "Theme", level: 3 })).toBeTruthy();
    expect(within(dialog).getByText("Remembered in this browser")).toBeTruthy();
  });

  it("shows one pane at a time, in the tab panel named by its tab", () => {
    renderSettings();
    const tab = screen.getByRole("tab", { name: "Reading" });

    fireEvent.click(tab);

    const panel = screen.getByRole("tabpanel", { name: "Reading" });
    expect(tab.getAttribute("aria-controls")).toBe(panel.id);
    expect(within(panel).getByRole("heading", { name: "Reading", level: 2 })).toBeTruthy();
    expect(within(panel).getByText("Remembered in this browser")).toBeTruthy();
    expect(within(panel).getByRole("heading", { name: "Face", level: 3 })).toBeTruthy();
    expect(
      within(panel).getByText("Document text only; titles and controls stay Alegreya Sans"),
    ).toBeTruthy();
    expect(screen.getByRole("radiogroup", { name: "Reading face" })).toBeTruthy();
    expect(screen.getByRole("radiogroup", { name: "Reading size" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Appearance" })).toBeNull();
    expect(screen.queryByRole("group", { name: "theme" })).toBeNull();
  });

  it("moves between tabs with the arrow keys, Home and End, choosing the pane on focus", () => {
    renderSettings();
    const tab = (name: string) => screen.getByRole("tab", { name });
    tab("Appearance").focus();

    fireEvent.keyDown(tab("Appearance"), { key: "ArrowDown" });
    expect(document.activeElement).toBe(tab("Reading"));
    expect(tab("Reading").getAttribute("aria-selected")).toBe("true");
    expect(tab("Reading").tabIndex).toBe(0);
    expect(tab("Appearance").tabIndex).toBe(-1);
    expect(screen.getByRole("heading", { name: "Reading", level: 2 })).toBeTruthy();

    fireEvent.keyDown(tab("Reading"), { key: "ArrowUp" });
    fireEvent.keyDown(tab("Appearance"), { key: "ArrowLeft" });
    expect(document.activeElement).toBe(tab("Summaries"));
    fireEvent.keyDown(tab("Summaries"), { key: "ArrowRight" });
    expect(document.activeElement).toBe(tab("Appearance"));
    fireEvent.keyDown(tab("Appearance"), { key: "End" });
    expect(document.activeElement).toBe(tab("Summaries"));
    fireEvent.keyDown(tab("Summaries"), { key: "Home" });
    expect(document.activeElement).toBe(tab("Appearance"));
    expect(screen.getByRole("heading", { name: "Appearance", level: 2 })).toBeTruthy();
  });

  it("uses all seven planner swatches at 120 by 76, hidden from assistive technology", () => {
    renderSettings();

    const theme = screen.getByRole("group", { name: "theme" });
    const swatches = theme.querySelectorAll('svg[width="120"][height="76"][aria-hidden="true"]');
    expect(swatches).toHaveLength(7);
    for (const label of [
      "Ledger",
      "Harbor",
      "Aubergine",
      "Graphite",
      "Midnight",
      "Obsidian",
      "Indigo Night",
    ]) {
      const card = within(theme).getByRole("button", { name: `${label} theme` });
      expect(card.querySelector("svg")?.getAttribute("viewBox")).toBe("0 0 120 76");
      expect(card.querySelector('[data-region="spine"]')).toBeTruthy();
    }
  });
});

describe("planner settings focus", () => {
  it.each(["close button", "Escape"] as const)(
    "returns focus to the opener after %s",
    async (method) => {
      render(
        <Provider store={createStore()}>
          <SettingsLauncher />
        </Provider>,
      );
      expect(screen.queryByRole("dialog")).toBeNull();
      const opener = screen.getByRole("button", { name: "Open settings" });
      opener.focus();

      fireEvent.click(opener);

      const dialog = screen.getByRole("dialog", { name: "Settings" });
      expect(dialog.contains(document.activeElement)).toBe(true);
      if (method === "Escape") fireEvent.keyDown(dialog, { key: "Escape" });
      else fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));

      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(opener));
    },
  );
});
