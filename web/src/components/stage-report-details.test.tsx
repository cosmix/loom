import { cleanup, fireEvent, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { fixtureStage, renderModal } from "./stage-report-kit";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("the details disclosure", () => {
  it("shows orchestrator usage and tokens while folded", () => {
    renderModal(
      fixtureStage("server", {
        status: "executing",
        context_tokens: 78000,
        context_ceiling_tokens: 200000,
      }),
      [],
    );

    const toggle = screen.getByRole("button", { name: "details" });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(within(toggle).getByText("orchestrator")).toBeTruthy();
    expect(within(toggle).getByText(`${(78000).toLocaleString()} tokens`)).toBeTruthy();
    expect(within(toggle).getByRole("img", { name: "context 39% of ceiling" })).toBeTruthy();
  });

  it("puts the chevron before the label", () => {
    renderModal(fixtureStage("server"), []);

    const toggle = screen.getByRole("button", { name: "details" });
    const chevron = toggle.querySelector(".disclosure-chevron");
    const label = within(toggle).getByText("details", { selector: ".eyebrow" });
    expect(chevron).toBeTruthy();
    expect(
      (chevron as Element).compareDocumentPosition(label) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("stays folded until opened, then lists the sections", () => {
    renderModal(fixtureStage("server"), []);

    const toggle = screen.getByRole("button", { name: "details" });
    expect(toggle.getAttribute("aria-describedby")).toBeTruthy();
    expect(screen.getByRole("button", { description: /type/ })).toBe(toggle);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(screen.queryByRole("region", { name: "identity" })).toBeNull();

    fireEvent.click(toggle);

    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(screen.getByRole("region", { name: "identity" })).toBeTruthy();
  });

  it("opens the sections inside the details card, under its header", () => {
    renderModal(fixtureStage("server"), []);

    const card = screen.getByRole("region", { name: "details" });
    fireEvent.click(within(card).getByRole("button", { name: "details" }));

    const identity = screen.getByRole("region", { name: "identity" });
    expect(card.contains(identity)).toBe(true);
    const panel = document.getElementById(
      within(card).getByRole("button", { name: "details" }).getAttribute("aria-controls") ?? "",
    );
    expect(panel?.parentElement).toBe(card);
    expect(panel?.contains(identity)).toBe(true);
  });
});
