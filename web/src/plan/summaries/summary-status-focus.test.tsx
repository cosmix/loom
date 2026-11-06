import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { expect, it } from "vitest";

import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { summaryPanelSchema } from "@/plan/api/schema-summaries";

import {
  answerPosts,
  deliver,
  fetchMock,
  handlers,
  idle,
  installHarness,
  question,
  show,
  showRow,
  visibleText,
} from "./summary-status-kit";

installHarness();

it("moves focus into the popover when the user opens it from the trigger", async () => {
  show();
  fireEvent.click(await screen.findByRole("button", { name: /Summaries up to date/ }));
  const dialog = await screen.findByRole("dialog", { name: "Summary rebuilds" });
  await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
});

it("reopens from the trigger with focus inside, and Escape returns focus to the trigger", async () => {
  show();
  const trigger = await screen.findByRole("button", { name: /Summaries up to date/ });
  const textbox = screen.getByRole("textbox", { name: "Message" });
  act(() => textbox.focus());
  deliver("summary_question", question);
  await screen.findByRole("button", { name: "Not now" });
  expect(document.activeElement).toBe(textbox);
  fireEvent.click(trigger);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Summary rebuilds" });
  await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
  fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape", code: "Escape" });
  await waitFor(() => expect(document.activeElement).toBe(trigger));
});

it("leaves focus in the composer when Escape closes a popover a question opened", async () => {
  show();
  await screen.findByRole("button", { name: /Summaries up to date/ });
  const textbox = screen.getByRole("textbox", { name: "Message" });
  act(() => textbox.focus());
  deliver("summary_question", question);
  await screen.findByRole("button", { name: "Not now" });
  fireEvent.keyDown(textbox, { key: "Escape", code: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  // Radix restores focus from a timeout once the content unmounts; let it run.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  expect(document.activeElement).toBe(textbox);
});

it("sends one answer when an answer button is clicked twice before the reply", async () => {
  show();
  await screen.findByRole("button", { name: /Summaries up to date/ });
  deliver("summary_question", question);
  let reply!: (response: Response) => void;
  fetchMock.mockImplementationOnce(() => new Promise<Response>((resolve) => (reply = resolve)));
  const notNow = await screen.findByRole("button", { name: "Not now" });
  fireEvent.click(notNow);
  fireEvent.click(notNow);
  await waitFor(() => expect(answerPosts()).toHaveLength(1));
  await act(async () => {});
  expect(answerPosts()).toHaveLength(1);
  await act(async () => {
    reply(
      new Response(JSON.stringify(idle), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
  });
  await waitFor(() => expect(screen.queryByRole("button", { name: "Not now" })).toBeNull());
});

it("the row variant labels the status, expands in place, and runs no feed of its own", () => {
  showRow({ status: "ready", panel: summaryPanelSchema.parse(panelFixture) });

  const row = screen.getByRole("button", { name: "Summary rebuilds: Rebuilding 1 of 3" });
  expect(visibleText(row)).toContain("Rebuilding 1 of 3");
  expect(visibleText(row)).toContain("1/3");
  // The tile leads the title "Summaries" (an item), so it is the live dot, not the arrows.
  expect(row.querySelector(".activity-tile .activity-glyph__led")).not.toBeNull();
  expect(row.querySelector(".activity-tile .plan-arrow-spinner")).toBeNull();
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("heading", { name: "Rebuilding 1 of 3" })).toBeNull();

  fireEvent.click(row);

  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByRole("heading", { name: "Rebuilding 1 of 3" })).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
  expect(handlers.size).toBe(0);
});

it("the row variant expands for an open question and leaves the announcing to its caller", () => {
  showRow({
    status: "ready",
    panel: summaryPanelSchema.parse({ ...panelFixture, running: null, question }),
  });

  expect(
    screen.getByRole("button", { name: /^Summary rebuilds:/ }).getAttribute("aria-expanded"),
  ).toBe("true");
  expect(screen.queryByRole("status")).toBeNull();
});
