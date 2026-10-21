import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { HelpDialog } from "@/components/help-dialog";
import { HELP_TOPICS } from "@/components/help-topics";
import { LEGEND } from "@/lib/states";
import { TerminalDemo, ResourceDiagram } from "@/components/help-session-demo";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("help guide", () => {
  it("uses the real terminal switch without connecting to a session", () => {
    const connect = vi.spyOn(globalThis, "WebSocket");
    render(<TerminalDemo />);
    const control = screen.getByRole("switch", { name: "Take control" });
    expect(control.getAttribute("aria-checked")).toBe("false");
    fireEvent.click(control);
    expect(control.getAttribute("aria-checked")).toBe("true");
    expect(document.querySelector(".terminal-well")?.getAttribute("data-mode")).toBe("control");
    expect(document.querySelector(".terminal-well .terminal-tape")).toBeTruthy();
    expect(screen.getByText(/In a live terminal, Esc and Tab/)).toBeTruthy();
    fireEvent.click(control);
    expect(control.getAttribute("aria-checked")).toBe("false");
    expect(document.querySelector(".terminal-well")?.getAttribute("data-mode")).toBe("view");
    expect(connect).not.toHaveBeenCalled();
  });

  it("renders production context, token and quota instruments with sample readings", () => {
    render(<ResourceDiagram />);
    expect(screen.getByRole("img", { name: "context 39% of ceiling" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Token flow" })).toBeTruthy();
    expect(screen.getByText("In the footer")).toBeTruthy();
    expect(screen.getByText("48%")).toBeTruthy();
  });

  it("navigates every topic and preserves the entire legend", () => {
    render(<HelpDialog open onOpenChange={() => {}} />);
    const nav = screen.getByRole("navigation", { name: "Help topics" });
    for (const topic of HELP_TOPICS) {
      fireEvent.click(within(nav).getByRole("button", { name: topic.label }));
      expect(screen.getByRole("heading", { level: 2, name: topic.label })).toBeTruthy();
    }
    fireEvent.click(within(nav).getByRole("button", { name: "Legend" }));
    for (const entry of LEGEND) expect(screen.getByText(entry.legend)).toBeTruthy();
    expect(screen.getByText("writing contract tests")).toBeTruthy();
    expect(screen.getByText(/resident tokens/)).toBeTruthy();
    expect(screen.getByText(/fresh tokens per minute/)).toBeTruthy();
  });

  it("pairs each illustration with its numbered notes", () => {
    render(<HelpDialog open onOpenChange={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Graph & navigation" }));
    const ledger = screen
      .getByRole("heading", { level: 3, name: "The ledger" })
      .closest("section")!;
    expect(within(ledger).getByRole("table")).toBeTruthy();
    expect(
      within(ledger)
        .getAllByRole("listitem")
        .map((note) => note.textContent),
    ).toEqual(HELP_TOPICS[1].scenes[1].notes.map((note, i) => `${i + 1}${note.text}`));
    expect(screen.queryByText("Write the docs")).toBeNull();
  });

  it("opens on the topic list on phones and returns from a topic", () => {
    vi.spyOn(window, "matchMedia").mockImplementation((query) => ({
      matches: query === "(max-width: 767px)",
      media: query,
      onchange: null,
      addListener() {},
      removeListener() {},
      addEventListener() {},
      removeEventListener() {},
      dispatchEvent: () => true,
    }));
    render(<HelpDialog open onOpenChange={() => {}} />);
    expect(screen.queryByRole("article")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Legend" }));
    expect(screen.queryByRole("navigation")).toBeNull();
    expect(screen.getByRole("article")).toBe(document.activeElement);
    fireEvent.click(screen.getByRole("button", { name: "All topics" }));
    expect(screen.getByRole("navigation", { name: "Help topics" })).toBeTruthy();
    expect(screen.queryByRole("article")).toBeNull();
  });

  it("starts a fresh visit at Getting started and closes on Escape", () => {
    const onOpenChange = vi.fn();
    const view = render(<HelpDialog open onOpenChange={onOpenChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Legend" }));
    view.rerender(<HelpDialog open={false} onOpenChange={onOpenChange} />);
    view.rerender(<HelpDialog open onOpenChange={onOpenChange} />);
    expect(screen.getByRole("heading", { name: "Getting started" })).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
