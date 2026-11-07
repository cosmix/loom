import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { KickoffShape } from "@/plan/api/schema-kickoff";
import { TakingShape } from "@/plan/new/taking-shape";

afterEach(cleanup);

const featureShape: KickoffShape = {
  size: "feature",
  name: "planner-search",
  proposal: true,
  problem: "Finding the right plan takes too long.",
  decisions: ["Search document titles.", "Keep results in the current repository."],
  questions: ["Should archived plans appear?"],
  sections: [
    { title: "Search scope", state: "settled" },
    { title: "Result ordering", state: "wording" },
    { title: "Archive handling", state: "open" },
  ],
};

describe("TakingShape", () => {
  it("shows the empty panel with undecided fields", () => {
    render(<TakingShape shape={null} target={null} />);

    const panel = screen.getByRole("complementary", { name: "Taking shape" });
    expect(within(panel).getByRole("heading", { name: "Taking shape" }).tagName).toBe("H2");
    expect(panel.classList.contains("font-sans")).toBe(true);
    expect(
      within(panel).getByText(
        "Nothing yet. The shape appears here as the conversation settles it.",
      ),
    ).toBeTruthy();
    expect(within(panel).getByText("Not named yet")).toBeTruthy();
    expect(within(panel).getByText("Not stated yet")).toBeTruthy();
    expect(within(panel).getByText("Nothing settled yet")).toBeTruthy();
    expect(within(panel).getByText("No open questions")).toBeTruthy();
    expect(within(panel).queryByRole("heading", { name: "Sections" })).toBeNull();
    expect(within(panel).queryByText(/Nothing is written yet/)).toBeNull();
    expect(within(panel).queryByRole("button", { name: /^Write the/ })).toBeNull();
  });

  it("shows a full feature and all three section states with distinct icons", () => {
    render(<TakingShape shape={featureShape} target={null} />);

    expect(screen.getByText("planner-search").parentElement?.classList.contains("font-mono")).toBe(
      true,
    );
    expect(screen.getByText("Finding the right plan takes too long.")).toBeTruthy();
    const decisions = within(screen.getByRole("region", { name: "Settled decisions" }));
    expect(decisions.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "Search document titles.",
      "Keep results in the current repository.",
    ]);
    const questions = within(screen.getByRole("region", { name: "Open questions" }));
    expect(questions.getByRole("listitem").textContent).toBe("Should archived plans appear?");
    const sections = within(screen.getByRole("region", { name: "Sections" }));
    expect(sections.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "01Search scopeSettled",
      "02Result orderingBeing worded",
      "03Archive handlingOpen",
    ]);
    expect(sections.getByText("Settled").querySelector(".lucide-circle-check")).not.toBeNull();
    expect(
      sections
        .getByText("Being worded")
        .querySelector('.lucide-refresh-cw.plan-arrow-spinner[data-kind="drafting"]'),
    ).not.toBeNull();
    expect(sections.getByText("Open").querySelector(".lucide-circle")).not.toBeNull();
  });

  it("hides proposed sections for a standalone plan", () => {
    render(<TakingShape shape={{ ...featureShape, size: "plan" }} target={null} />);

    expect(screen.getByRole("button", { name: "Write the plan" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Sections" })).toBeNull();
    expect(screen.queryByText("Search scope")).toBeNull();
  });

  it("states where the named work will be written and enables writing", () => {
    render(<TakingShape shape={featureShape} target="doc/product/planner-search/index.md" />);

    const target = screen.getByText("doc/product/planner-search/index.md");
    expect(target.closest("p")?.textContent).toBe(
      "Nothing is written yet. 'Write the vision' creates doc/product/planner-search/index.md with the open questions in it.",
    );
    expect(target.closest("code")).not.toBeNull();
  });

  it("drops the path from the notice and blocks writing for a reader without authoring", () => {
    render(<TakingShape shape={featureShape} target={null} />);

    expect(screen.getByText(/Nothing is written yet/).textContent).toBe(
      "Nothing is written yet. 'Write the vision' confirms the name and creates the vision with the open questions in it.",
    );
    expect(
      screen.getByRole("button", { name: "Write the vision" }).getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("starts at the Name row and renders a proposal slot above the rows", () => {
    render(
      <TakingShape shape={featureShape} target={null}>
        <div role="region" aria-label="Proposal" />
      </TakingShape>,
    );

    const panel = screen.getByRole("complementary", { name: "Taking shape" });
    expect(panel.querySelector(":scope > section:nth-of-type(1) h3")?.textContent).toBe("Name");
    expect(panel.querySelector(":scope > section:nth-of-type(5) h3")?.textContent).toBe("Sections");
    expect(within(panel).queryByRole("heading", { name: "Size" })).toBeNull();
    expect(
      panel
        .querySelector('[aria-label="Proposal"]')!
        .compareDocumentPosition(panel.querySelector(":scope > section:nth-of-type(1)")!) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0);
  });

  it("shows placeholders for a shape whose fields are still empty", () => {
    const shape: KickoffShape = {
      size: null,
      name: null,
      proposal: false,
      problem: "",
      decisions: [],
      questions: [],
      sections: [],
    };

    render(<TakingShape shape={shape} target={null} />);

    expect(screen.getByText("Not named yet")).toBeTruthy();
    expect(screen.getByText("Not stated yet")).toBeTruthy();
    expect(screen.getByText("Nothing settled yet")).toBeTruthy();
    expect(screen.getByText("No open questions")).toBeTruthy();
  });

  it("shows a placeholder when a feature has no proposed sections", () => {
    render(<TakingShape shape={{ ...featureShape, sections: [] }} target={null} />);

    const sections = within(screen.getByRole("region", { name: "Sections" }));
    expect(sections.getByText("No sections proposed yet")).toBeTruthy();
    expect(sections.queryByRole("list")).toBeNull();
  });

  it("renders hostile text through Untrusted in every supplied text field", () => {
    const override = String.fromCodePoint(0x202e);
    const hostile = `<b>${override}path</b>`;
    const shape: KickoffShape = {
      ...featureShape,
      name: hostile,
      problem: hostile,
      decisions: [hostile],
      questions: [hostile],
      sections: [{ title: hostile, state: "open" }],
    };

    render(<TakingShape shape={shape} target={hostile} />);

    const panel = screen.getByRole("complementary", { name: "Taking shape" });
    expect(panel.textContent).not.toContain(override);
    const suppliedText = within(panel).getAllByText("<b>⟨U+202E⟩path</b>");
    expect(suppliedText).toHaveLength(6);
    for (const text of suppliedText) {
      expect(text.style.unicodeBidi).toBe("isolate");
      expect(text.style.direction).toBe("ltr");
      expect(text.childElementCount).toBe(0);
    }
  });
});

describe("the Name row", () => {
  it("only displays the name: nothing to rename, name or change size with", () => {
    for (const [shape, target] of [
      [null, null],
      [featureShape, null],
      [{ ...featureShape, name: null }, null],
      [featureShape, "doc/product/planner-search/index.md"],
    ] as const) {
      render(<TakingShape shape={shape} target={target} />);
      const panel = within(screen.getByRole("complementary", { name: "Taking shape" }));
      expect(panel.queryByRole("button", { name: /^(Rename|Name it|Use this)$/ })).toBeNull();
      expect(panel.queryByRole("button", { name: /^(Feature|Standalone plan)$/ })).toBeNull();
      expect(panel.queryByRole("textbox")).toBeNull();
      cleanup();
    }
  });

  it("shows the name as a chip, and a placeholder while there is none", () => {
    render(<TakingShape shape={featureShape} target={null} />);
    expect(screen.getByText("planner-search").closest(".kickoff-chip")).not.toBeNull();
    cleanup();
    render(<TakingShape shape={{ ...featureShape, name: null }} target={null} />);
    expect(screen.getByText("Not named yet")).toBeTruthy();
  });
});
