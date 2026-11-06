import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { expect, it } from "vitest";

import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import { connectionEpochAtom } from "@/plan/state/atoms";

import {
  PANEL_PATH,
  deliver,
  fetchMock,
  handlers,
  idle,
  installHarness,
  question,
  resyncs,
  setReplies,
  show,
  visibleText,
} from "./summary-status-kit";

installHarness();

it("shows the idle label and subscribes to the panel feed once", async () => {
  show();
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Summaries up to date",
  });
  expect(visibleText(trigger)).toBe("Summaries up to date");
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(fetchMock).toHaveBeenCalledWith(PANEL_PATH, expect.objectContaining({ method: "GET" }));
  expect(handlers.get("summary_question")?.size).toBe(1);
  expect(resyncs.size).toBe(1);
});

it("shows progress while a rebuild is running", async () => {
  setReplies([{ status: 200, body: panelFixture }]);
  show();
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Rebuilding 1 of 3",
  });
  expect(visibleText(trigger)).toBe("Rebuilding 1 of 3");
  fireEvent.click(trigger);
  expect(await screen.findByRole("dialog", { name: "Summary rebuilds" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "Rebuilding 1 of 3" })).toBeTruthy();
});

it("shows a compact glyph and progress count with a full accessible name", async () => {
  setReplies([{ status: 200, body: panelFixture }]);
  show(true);
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Rebuilding 1 of 3",
  });
  expect(trigger.querySelector('.plan-arrow-spinner[aria-hidden="true"]')).toBeTruthy();
  expect(visibleText(trigger)).toBe("1/3");
  expect(trigger.getAttribute("aria-label")).toBe("Summary rebuilds: Rebuilding 1 of 3");
});

it("keeps the compact idle control to a glyph", async () => {
  show(true);
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Summaries up to date",
  });
  expect(trigger.querySelector("svg")).toBeTruthy();
  expect(visibleText(trigger)).toBe("");
});

it("draws the compact glyph as the summaries' state: the two rotating arrows while rebuilding", async () => {
  setReplies([{ status: 200, body: panelFixture }]);
  show(true);
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Rebuilding 1 of 3",
  });
  expect(trigger.querySelector('svg.plan-arrow-spinner[aria-hidden="true"]')).toBeTruthy();
  expect(trigger.querySelector(".plan-arrow-spinner")?.getAttribute("data-busy")).toBe("true");
  expect(trigger.querySelector(".lucide-refresh-cw.plan-arrow-spinner")).toBeTruthy();
  expect(visibleText(trigger)).toBe("1/3");
});

it("draws the compact glyph as the summaries' state: the check when all are current", async () => {
  show(true);
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Summaries up to date",
  });
  expect(trigger.querySelector("svg.lucide-circle-check")).toBeTruthy();
  expect(trigger.querySelector(".plan-arrow-spinner")).toBeNull();
});

it("deduplicates stale IDs across queued, failed, and questioned summaries", async () => {
  setReplies([
    {
      status: 200,
      body: {
        ...idle,
        queue: ["a", "a", "b"],
        failed: [{ id: "b", error: "failed" }],
        question: { ...question, ids: ["b", "c"], count: 3 },
      },
    },
  ]);
  show(true);
  const trigger = await screen.findByRole("button", { name: "Summary rebuilds: 4 out of date" });
  expect(visibleText(trigger)).toBe("4");
});

it("keeps a trigger during loading and shows the loading state when opened", () => {
  fetchMock.mockImplementation(() => new Promise<Response>(() => {}));
  show();
  fireEvent.click(screen.getByRole("button", { name: "Summary rebuilds: Loading summaries" }));
  expect(screen.getByRole("status", { name: "Loading summaries" })).toBeTruthy();
});

it("shows a panel fetch error and retries from the popover", async () => {
  setReplies([
    { status: 500, body: { error: "internal", message: "panel failed\u202E" } },
    { status: 200, body: idle },
  ]);
  show();
  fireEvent.click(await screen.findByRole("button", { name: /status unavailable/ }));
  expect(await screen.findByText("panel failed⟨U+202E⟩")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await screen.findByRole("button", { name: "Summary rebuilds: Summaries up to date" });
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(screen.queryByText("panel failed⟨U+202E⟩")).toBeNull();
});

it("opens an arriving question and announces it without taking focus or answering", async () => {
  show();
  await screen.findByRole("button", { name: /Summaries up to date/ });
  const textbox = screen.getByRole("textbox", { name: "Message" });
  act(() => textbox.focus());
  const live = document.querySelector('div.sr-only[role="status"]');
  expect(live?.textContent).toBe("");
  deliver("summary_question", question);
  await screen.findByRole("button", { name: "Not now" });
  expect(document.activeElement).toBe(textbox);
  expect(live?.textContent).toBe("Rebuild 6 summaries? Estimated 18,400 tokens.");
  fireEvent.keyDown(textbox, { key: "Enter", code: "Enter" });
  fireEvent.keyUp(textbox, { key: "Enter", code: "Enter" });
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("does not reopen a dismissed question until a new ID arrives", async () => {
  show();
  const trigger = await screen.findByRole("button", { name: /Summaries up to date/ });
  deliver("summary_question", question);
  await screen.findByRole("button", { name: "Not now" });
  fireEvent.click(trigger);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  deliver("summary_question", { ...question });
  expect(screen.queryByRole("dialog")).toBeNull();
  deliver("summary_question", { ...question, id: "another-question" });
  await screen.findByRole("button", { name: "Not now" });
});

it("reseeds the panel and clears the question after a reconnect", async () => {
  setReplies([
    { status: 200, body: panelFixture },
    { status: 200, body: idle },
  ]);
  const store = show();
  await screen.findByRole("button", { name: /Rebuilding 1 of 3/ });
  deliver("summary_question", question);
  await screen.findByRole("button", { name: "Not now" });
  act(() => {
    store.set(connectionEpochAtom, (epoch) => epoch + 1);
    resyncs.forEach((handler) => handler());
  });
  await screen.findByRole("button", { name: /Summaries up to date/ });
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("button", { name: "Not now" })).toBeNull();
  expect(document.querySelector('div.sr-only[role="status"]')?.textContent).toBe("");
});

it("shows a failed answer and sends the chosen question ID and choice", async () => {
  show();
  await screen.findByRole("button", { name: /Summaries up to date/ });
  deliver("summary_question", question);
  fetchMock.mockResolvedValueOnce(
    new Response(JSON.stringify({ error: "internal", message: "answer failed\u202E" }), {
      status: 500,
      headers: { "content-type": "application/json" },
    }),
  );
  fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
  const dialog = screen.getByRole("dialog", { name: "Summary rebuilds" });
  expect((await within(dialog).findByRole("alert")).textContent).toBe("answer failed⟨U+202E⟩");
  expect(fetchMock).toHaveBeenLastCalledWith(
    expect.stringContaining("/api/summaries/answer"),
    expect.objectContaining({
      method: "POST",
      body: JSON.stringify({ question: question.id, rebuild: false }),
    }),
  );
  fetchMock.mockResolvedValueOnce(
    new Response(JSON.stringify(idle), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  );
  fireEvent.click(screen.getByRole("button", { name: "Rebuild all" }));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Not now" })).toBeNull());
  expect(screen.queryByText("answer failed⟨U+202E⟩")).toBeNull();
});

it("shows the question to a browser that cannot answer", async () => {
  show(false, false);
  await screen.findByRole("button", { name: /Summaries up to date/ });
  deliver("summary_question", question);
  const rebuild = await screen.findByRole("button", { name: "Rebuild all" });
  expect(rebuild.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(rebuild);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});
