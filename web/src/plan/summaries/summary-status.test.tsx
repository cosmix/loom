import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import { initClient } from "@/plan/api/client";
import helloFixture from "@/plan/api/fixtures/hello.json";
import panelFixture from "@/plan/api/fixtures/summary-panel.json";
import questionFixture from "@/plan/api/fixtures/summary-question.json";
import { serverInfoSchema } from "@/plan/api/schema";
import { summaryPanelSchema, summaryQuestionSchema } from "@/plan/api/schema-summaries";
import type { connectPlanSocket } from "@/plan/api/socket";
import {
  authoringAtom,
  connectionEpochAtom,
  planSocketAtom,
  serverInfoAtom,
  youAtom,
} from "@/plan/state/atoms";

import { panelLoadAtom, type PanelLoad } from "./state";
import { SummaryStatus, SummaryStatusRow } from "./summary-status";

type Message = Record<string, unknown>;
type Handler = (message: Message) => void;
const PANEL_PATH = "/api/summaries/panel";
const idle = summaryPanelSchema.parse({
  ...panelFixture,
  running: null,
  queue: [],
  failed: [],
  question: null,
  terminal_claims: [],
});
const question = summaryQuestionSchema.parse(questionFixture);
let handlers: Map<string, Set<Handler>>;
let resyncs: Set<() => void>;
let replies: Array<{ status: number; body: unknown }>;
let fetchMock: ReturnType<typeof vi.fn<typeof fetch>>;

function fakeSocket(): ReturnType<typeof connectPlanSocket> {
  const subscribe = (type: string, handler: Handler) => {
    const listeners = handlers.get(type) ?? new Set<Handler>();
    handlers.set(type, listeners.add(handler));
    return () => void listeners.delete(handler);
  };
  const onResync = (handler: () => void) => {
    resyncs.add(handler);
    return () => void resyncs.delete(handler);
  };
  return { subscribe, onResync, send: () => true, dispose: () => {} } as unknown as ReturnType<
    typeof connectPlanSocket
  >;
}

function seededStore(canAnswer: boolean) {
  const store = createStore();
  initClient(store);
  store.set(youAtom, helloFixture.you);
  store.set(serverInfoAtom, serverInfoSchema.parse({ ...helloFixture.server, read_only: [] }));
  store.set(authoringAtom, {
    holder: {
      ...(canAnswer ? helloFixture.you : { public_id: "other-browser", name: "Dana" }),
      away_since: null,
      frees_at: null,
    },
  });
  store.set(planSocketAtom, fakeSocket());
  return store;
}

function showRow(load: PanelLoad) {
  const store = seededStore(true);
  store.set(panelLoadAtom, load);
  render(
    <Provider store={store}>
      <TooltipProvider>
        <SummaryStatusRow load={load} reload={() => {}} />
      </TooltipProvider>
    </Provider>,
  );
  return store;
}

function show(compact = false, canAnswer = true) {
  const store = seededStore(canAnswer);
  render(
    <Provider store={store}>
      <TooltipProvider>
        <textarea aria-label="Message" />
        <SummaryStatus compact={compact} />
      </TooltipProvider>
    </Provider>,
  );
  return store;
}

function deliver(type: string, body: Message) {
  act(() => handlers.get(type)?.forEach((handler) => handler({ type, ...body })));
}

function visibleText(element: HTMLElement): string | null {
  const clone = element.cloneNode(true) as HTMLElement;
  clone.querySelectorAll(".sr-only").forEach((hidden) => hidden.remove());
  return clone.textContent;
}

beforeEach(() => {
  handlers = new Map();
  resyncs = new Set();
  replies = [{ status: 200, body: idle }];
  fetchMock = vi.fn<typeof fetch>(async () => {
    const reply = replies.length > 1 ? replies.shift() : replies[0];
    return new Response(JSON.stringify(reply?.body), {
      status: reply?.status ?? 500,
      headers: { "content-type": "application/json" },
    });
  });
  vi.stubGlobal("fetch", fetchMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

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
  replies = [{ status: 200, body: panelFixture }];
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
  replies = [{ status: 200, body: panelFixture }];
  show(true);
  const trigger = await screen.findByRole("button", {
    name: "Summary rebuilds: Rebuilding 1 of 3",
  });
  expect(trigger.querySelector('svg[aria-hidden="true"]')).toBeTruthy();
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

it("deduplicates stale IDs across queued, failed, and questioned summaries", async () => {
  replies = [
    {
      status: 200,
      body: {
        ...idle,
        queue: ["a", "a", "b"],
        failed: [{ id: "b", error: "failed" }],
        question: { ...question, ids: ["b", "c"], count: 3 },
      },
    },
  ];
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
  replies = [
    { status: 500, body: { error: "internal", message: "panel failed" } },
    { status: 200, body: idle },
  ];
  show();
  fireEvent.click(await screen.findByRole("button", { name: /status unavailable/ }));
  expect(await screen.findByText("panel failed")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  await screen.findByRole("button", { name: "Summary rebuilds: Summaries up to date" });
  expect(fetchMock).toHaveBeenCalledTimes(2);
  expect(screen.queryByText("panel failed")).toBeNull();
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
  replies = [
    { status: 200, body: panelFixture },
    { status: 200, body: idle },
  ];
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
    new Response(JSON.stringify({ error: "internal", message: "answer failed" }), {
      status: 500,
      headers: { "content-type": "application/json" },
    }),
  );
  fireEvent.click(await screen.findByRole("button", { name: "Not now" }));
  const dialog = screen.getByRole("dialog", { name: "Summary rebuilds" });
  expect((await within(dialog).findByRole("alert")).textContent).toBe("answer failed");
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
  expect(screen.queryByText("answer failed")).toBeNull();
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

function answerPosts() {
  return fetchMock.mock.calls.filter(([url]) => String(url).includes("/api/summaries/answer"));
}

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
  expect(row.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("heading", { name: "Rebuilding 1 of 3" })).toBeNull();

  fireEvent.click(row);

  expect(row.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByRole("heading", { name: "Rebuilding 1 of 3" })).toBeTruthy();
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(fetchMock).not.toHaveBeenCalled();
  expect(handlers.size).toBe(0);
});

it("the row variant expands for an open question and announces it", () => {
  showRow({
    status: "ready",
    panel: summaryPanelSchema.parse({ ...panelFixture, running: null, question }),
  });

  expect(
    screen.getByRole("button", { name: /^Summary rebuilds:/ }).getAttribute("aria-expanded"),
  ).toBe("true");
  expect(screen.getByRole("status").textContent).toContain(`Rebuild ${question.count} summaries?`);
});
