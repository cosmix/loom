import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { Provider } from "jotai";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { KickoffShape } from "@/plan/api/schema-kickoff";
import type { connectPlanSocket } from "@/plan/api/socket";
import { storeFor } from "@/plan/chat/panel-test-kit";
import { TakingShape } from "@/plan/new/taking-shape";
import { planSocketAtom } from "@/plan/state/atoms";

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
    expect(within(panel).getByText("Not decided yet")).toBeTruthy();
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

    expect(screen.getByRole("button", { name: "Feature" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(
      screen.getByRole("button", { name: "Standalone plan" }).getAttribute("aria-pressed"),
    ).toBe("false");
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

    expect(
      screen.getByRole("button", { name: "Standalone plan" }).getAttribute("aria-pressed"),
    ).toBe("true");
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

  it("drops the path from the notice and blocks writing while no name is settled", () => {
    render(<TakingShape shape={featureShape} target={null} />);

    expect(screen.getByText(/Nothing is written yet/).textContent).toBe(
      "Nothing is written yet. 'Write the vision' creates the vision with the open questions in it.",
    );
    expect(
      screen.getByRole("button", { name: "Write the vision" }).getAttribute("aria-disabled"),
    ).toBe("true");
  });

  it("keeps the Size row the first section and renders a proposal slot above the rows", () => {
    render(
      <TakingShape shape={featureShape} target={null}>
        <div role="region" aria-label="Proposal" />
      </TakingShape>,
    );

    const panel = screen.getByRole("complementary", { name: "Taking shape" });
    expect(panel.querySelector(":scope > section:nth-of-type(1) h3")?.textContent).toBe("Size");
    expect(panel.querySelector(":scope > section:nth-of-type(6) h3")?.textContent).toBe("Sections");
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

    expect(screen.getByText("Not decided yet")).toBeTruthy();
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
  function mountRename(shape: KickoffShape | null, target: string | null = null) {
    const store = storeFor();
    const listeners: Array<(message: unknown) => void> = [];
    const send = vi.fn((_message: unknown) => true);
    const socket = {
      send,
      subscribe: (type: string, handler: (message: unknown) => void) => {
        if (type === "chat") listeners.push(handler);
        return () => {};
      },
      onResync: () => () => {},
      dispose: () => {},
    };
    store.set(planSocketAtom, socket as unknown as ReturnType<typeof connectPlanSocket>);
    render(
      <Provider store={store}>
        <TakingShape shape={shape} target={target} />
      </Provider>,
    );
    const answer = async (error: string | null) => {
      const sent = send.mock.calls.at(-1)?.[0] as { client_id: string };
      const frame = { type: "chat", frame: "result", client_id: sent.client_id };
      const result = { ...frame, ok: !error, error };
      await act(async () => listeners.forEach((handler) => handler(result)));
    };
    return { send, answer };
  }

  it("offers Rename once a shape exists and Name it while the name is empty", () => {
    mountRename(featureShape);
    expect(screen.getByRole("button", { name: "Rename" })).toBeTruthy();
    cleanup();
    mountRename({ ...featureShape, name: null });
    expect(screen.getByRole("button", { name: "Name it" })).toBeTruthy();
  });

  it("offers nothing before any shape and nothing once the work is confirmed", () => {
    mountRename(null);
    expect(screen.queryByRole("button", { name: /^(Rename|Name it)$/ })).toBeNull();
    cleanup();
    mountRename(featureShape, "doc/product/planner-search/index.md");
    expect(screen.queryByRole("button", { name: /^(Rename|Name it)$/ })).toBeNull();
  });

  it("sends rename_kickoff for a valid name and closes the field when it is accepted", async () => {
    const { send, answer } = mountRename(featureShape);
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "better-name" } });
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(send).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ command: "rename_kickoff", name: "better-name" }),
    );
    await answer(null);
    expect(screen.queryByRole("textbox")).toBeNull();
  });

  it("refuses an invalid name and cancels on Escape", () => {
    const { send } = mountRename(featureShape);
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "Not A Name" } });
    expect(screen.getByRole("alert")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(send).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Escape" });
    expect(screen.queryByRole("textbox")).toBeNull();
    expect(screen.getByRole("button", { name: "Rename" })).toBeTruthy();
  });

  it("keeps the field open and shows the server's refusal", async () => {
    const { answer } = mountRename(featureShape);
    fireEvent.click(screen.getByRole("button", { name: "Rename" }));
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "better-name" } });
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    await answer("The name is fixed once the work is confirmed");
    expect(screen.getByRole("status").textContent).toBe(
      "The name is fixed once the work is confirmed",
    );
    expect(screen.getByRole("textbox")).toBeTruthy();
  });
});
