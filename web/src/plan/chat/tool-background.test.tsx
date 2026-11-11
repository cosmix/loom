import { cleanup, fireEvent, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { connectBackground, stubFetch, TASKS } from "./background-feed-kit";
import { item, mount, turn } from "./transcript-test-kit";

type Row = ReturnType<typeof tool>;

const [AGENT, , STALLED, , SHELL_FAILED] = TASKS as [
  (typeof TASKS)[number],
  (typeof TASKS)[number],
  (typeof TASKS)[number],
  (typeof TASKS)[number],
  (typeof TASKS)[number],
];
const text = (element: Element | null): string =>
  (element?.textContent ?? "").replace(/\s+/g, " ").trim();

let disposers: Array<() => void> = [];

beforeEach(() => {
  stubFetch();
});

afterEach(() => {
  cleanup();
  disposers.splice(0).forEach((dispose) => dispose());
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function tool(id: string, fields: Partial<ReturnType<typeof item<"tool">>> = {}) {
  return item("tool", {
    id: `item-${id}`,
    tool_use_id: `toolu_${id}`,
    status: "done",
    summary: `Row ${id}`,
    ...fields,
  });
}

/** A closed turn of `items` on a store wired to the fake plan socket. */
async function mountClosed(items: Row[]) {
  const view = mount(items, { turns: [turn(items, { open: false })] });
  const background = await connectBackground(view.store);
  disposers.push(background.dispose);
  return { ...view, feed: background.feed };
}

const head = (log: HTMLElement, summary: string): HTMLElement =>
  within(log).getByRole("button", { name: new RegExp(summary), hidden: true });
const folded = (log: HTMLElement, summary: string): boolean =>
  head(log, summary).closest("details.chat-tool-fold") !== null;

it("an agent row shows its task's words, stays out of the fold while it runs, and folds once done", async () => {
  const agent = tool("agent", { name: "Agent", tool_use_id: AGENT.tool_use_id! });
  const { log, feed } = await mountClosed([tool("read1"), tool("read2"), agent]);
  expect(folded(log, "Row agent")).toBe(true);
  expect(text(head(log, "Row agent"))).toMatch(/Done/);

  feed([AGENT]);
  await vi.waitFor(() => expect(folded(log, "Row agent")).toBe(false));
  expect(text(head(log, "Row agent"))).toMatch(/\brunning\b/);
  expect(text(head(log, "Row agent"))).not.toMatch(/\bdone\b/i);
  expect(head(log, "Row agent").querySelector(".chat-tool-status svg")).not.toBeNull();
  expect(folded(log, "Row read1")).toBe(true);

  feed([{ ...AGENT, state: "done" }]);
  await vi.waitFor(() => expect(folded(log, "Row agent")).toBe(true));
  expect(text(head(log, "Row agent"))).toMatch(/\bdone\b/);
});

it("a stalled task's row says stalled? and stays outside the fold", async () => {
  const quiet = tool("quiet", { name: "Agent", tool_use_id: STALLED.tool_use_id! });
  const { log, feed } = await mountClosed([tool("read1"), quiet]);
  feed([STALLED]);
  await vi.waitFor(() => expect(text(head(log, "Row quiet"))).toMatch(/stalled\?/));
  expect(folded(log, "Row quiet")).toBe(false);
});

it("a failed and a stopped shell row show their words and fold", async () => {
  const failed = tool("failed", { name: "Bash", tool_use_id: SHELL_FAILED.tool_use_id! });
  const stopped = tool("stopped", { name: "Bash", tool_use_id: "toolu_stop" });
  const { log, feed } = await mountClosed([failed, stopped]);
  feed([
    SHELL_FAILED,
    { ...SHELL_FAILED, id: "bshell03", tool_use_id: "toolu_stop", state: "stopped" },
  ]);

  await vi.waitFor(() => expect(text(head(log, "Row failed"))).toMatch(/\bfailed\b/));
  expect(text(head(log, "Row stopped"))).toMatch(/\bstopped\b/);
  expect(
    head(log, "Row failed").querySelector(".chat-tool-status")?.getAttribute("data-status"),
  ).toBe("error");
  expect(folded(log, "Row failed")).toBe(true);
  expect(folded(log, "Row stopped")).toBe(true);
});

it("a Task row nests its subagent's tool calls like an Agent row", async () => {
  const parent = tool("parent", { name: "Task", nested_tools: 1 });
  const child = tool("child", { subagent: "explorer", parent: "toolu_parent" });
  const { log } = await mountClosed([parent, child]);

  expect(text(head(log, "Row parent"))).toMatch(/1 tool call/);
  expect(log.querySelectorAll(".chat-tool-nested").length).toBe(0);
  fireEvent.click(head(log, "Row parent"));
  const nested = log.querySelector(".chat-tool-nested");
  expect(nested).not.toBeNull();
  expect(within(nested as HTMLElement).getByRole("button", { name: /Row child/ })).toBeTruthy();
  // The nested call is not also drawn as a top-level row: the parent's and the child's heads only.
  expect(log.querySelectorAll(".chat-tool-head").length).toBe(2);
});

it("a Bash row without a task id folds as today", async () => {
  const { log, feed } = await mountClosed([tool("bash", { name: "Bash" }), tool("read1")]);
  feed([AGENT]);
  await vi.waitFor(() => expect(text(head(log, "Row bash"))).toMatch(/Done/));
  expect(folded(log, "Row bash")).toBe(true);
  expect(log.querySelectorAll("details.chat-tool-fold").length).toBe(1);
});
