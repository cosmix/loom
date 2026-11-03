import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";

import brief from "@/plan/api/fixtures/session-brief.json";
import transcriptCss from "./chat-transcript.css?raw";
import { snapshotAtom } from "./model";
import { fixture, item, mount, turn } from "./transcript-test-kit";

const clipboardDescriptor = Object.getOwnPropertyDescriptor(navigator, "clipboard");

beforeEach(() => {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(JSON.stringify(brief))),
  );
});

afterEach(() => {
  cleanup();
  if (clipboardDescriptor) Object.defineProperty(navigator, "clipboard", clipboardDescriptor);
  else Reflect.deleteProperty(navigator, "clipboard");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("user item layout and chips", () => {
  const attachment = item("user").attachments[0]!;
  const { log } = mount([
    item("user", { text: "First line\nSecond line", origin: null, attachments: [attachment] }),
  ]);
  const text = within(log).getByText(/First line/);
  expect(text.textContent).toBe("First line\nSecond line");
  expect(text.closest('[class*="whitespace-pre-wrap"]')).not.toBeNull();
  expect(text.closest(".chat-user-body")).not.toBeNull();
  expect(text.closest(".chat-user-body")?.firstElementChild?.className).toBe("chat-user-quote");
  expect(log.querySelector(".chat-user")).not.toBeNull();
  expect(within(log).getByRole("button", { name: /reference-0.md/ })).toBeTruthy();
});

it("thinking markers merge", () => {
  const { log } = mount([
    item("thinking", { id: "think1", duration_ms: 2000 }),
    item("thinking", { id: "think2", duration_ms: 3000 }),
    item("text", { markdown: "Done" }),
  ]);
  expect(within(log).getAllByText(/Thought for/)).toHaveLength(1);
  expect(within(log).getByText(/Thought for 5s/)).toBeTruthy();
});

it.each<[string, Array<number | null>]>([
  ["a later", [2000, null]],
  ["an earlier", [null, 3000]],
  ["a middle", [2000, null, 3000]],
])("%s thinking marker without a duration leaves the merged duration unknown", (_, durations) => {
  const markers = durations.map((duration_ms, index) =>
    item("thinking", { id: `think${index}`, duration_ms }),
  );
  const { log } = mount([...markers, item("text", { markdown: "Done" })]);
  expect(within(log).queryByText(/Thought for/)).toBeNull();
  expect(within(log).getByText("Thought")).toBeTruthy();
});

it("api_error notice toned blocked", () => {
  const { log } = mount([item("notice", { kind: "api_error", text: "API unavailable" })]);
  // Only the glyph takes the tone (DESIGN.md, Colour); the words stay in the text's colour.
  expect(within(log).getByText("API unavailable").closest('[class*="tone-blocked"]')).toBeNull();
  const glyph = within(log).getByRole("img", { name: "API error" });
  expect(glyph.className).toContain("chat-notice-glyph");
  // The tone reaches the glyph only through this CSS rule, so pin the attribute and the rule.
  expect(glyph.closest(".chat-notice-item")?.getAttribute("data-kind")).toBe("api_error");
  expect(transcriptCss).toMatch(
    /\.chat-notice-item\[data-kind="api_error"\] \.chat-notice-glyph \{\s*color: var\(--tone-blocked\);/,
  );
});

it("stopped notice", () => {
  const { log } = mount([item("notice", { kind: "stopped", text: "Session stopped" })]);
  expect(within(log).getByText("Session stopped")).toBeTruthy();
  expect(log.querySelector('[class*="text-center"]')).not.toBeNull();
});

it("html comment dropped", () => {
  const { log } = mount([
    item("text", { markdown: "Before\n\n<!-- private comment -->\n\nAfter" }),
  ]);
  expect(log.textContent).toContain("Before");
  expect(log.textContent).toContain("After");
  expect(log.textContent).not.toContain("private comment");
  expect(log.querySelector("script, img")).toBeNull();
});

it("code block shows language label and copy control", () => {
  const { log } = mount([item("text", { markdown: "```rust\nfn main() {}\n```" })]);
  expect(within(log).getByText("rust")).toBeTruthy();
  expect(within(log).getByRole("button", { name: "Copy rust code" })).toBeTruthy();
  expect(log.querySelector("pre")?.textContent).toContain("fn main() {}");
});

it("doc link opens file mode", () => {
  const { log, router } = mount([item("text", { markdown: "[Read](/doc/product/section_02.md)" })]);
  fireEvent.click(within(log).getByRole("link", { name: "Read" }));
  expect(router.state.location.pathname).toBe("/files/product/section_02.md");
});

it("outside-doc path is text", () => {
  const { log } = mount([item("text", { markdown: "[Source](../src/main.rs)" })]);
  expect(within(log).queryByRole("link", { name: /Source/ })).toBeNull();
  expect(log.textContent).toContain("Source");
});

it("http link opens new tab with noopener", () => {
  const { log } = mount([item("text", { markdown: "[External](https://example.com/read)" })]);
  const link = within(log).getByRole("link", { name: "External" });
  expect(link.getAttribute("target")).toBe("_blank");
  expect(link.getAttribute("rel")).toBe("noopener noreferrer");
});

it("no placeholder while working", () => {
  const { log } = mount();
  expect(log.querySelector(".md")).toBeNull();
  expect(log.textContent).not.toMatch(/thinking|working|typing|\.\.\./i);
});

it("copy control copies markdown source", async () => {
  const writeText = vi.fn().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
  const markdown = "**Bold** and `code`\n\n[external](https://example.com)";
  const { log } = mount([item("text", { markdown })]);
  fireEvent.click(
    within(log).getByRole("button", { name: /copy.*message|copy.*markdown|^copy$/i }),
  );
  await waitFor(() => expect(writeText).toHaveBeenCalledWith(markdown));
});

it("offer origin collapsed with brief label", async () => {
  const { log } = mount([
    item("user", { origin: "offer", attachments: [], text: "Opening line\nHidden instructions" }),
  ]);
  await within(log).findByText(brief.label);
  expect(within(log).queryByText(/Opening line|Hidden instructions/)).toBeNull();
  fireEvent.click(within(log).getByRole("button", { name: /show/i }));
  expect(within(log).getByText(/Hidden instructions/)).toBeTruthy();
});

it("closing origin labelled Closing message from loom plan", () => {
  const { log } = mount([
    item("user", { origin: "closing", attachments: [], text: "Close\nHidden close" }),
  ]);
  expect(within(log).getByText("Closing message from loom plan")).toBeTruthy();
  expect(within(log).queryByText(/Hidden close/)).toBeNull();
});

it("null origin renders plainly", () => {
  const { log } = mount([
    item("user", { origin: null, attachments: [], text: "My first line\nMy second line" }),
  ]);
  expect(within(log).getByText(/My second line/)).toBeTruthy();
  expect(within(log).queryByRole("button", { name: /show/i })).toBeNull();
});

it("older turns collapse to one line and expand", () => {
  const turns = Array.from({ length: 21 }, (_, index) =>
    turn([item("text", { id: `text-${index}`, markdown: `Reply ${index + 1}` })], {
      id: `t${index + 1}`,
      started_at: new Date(index * 60000).toISOString(),
      open: false,
      // Cache reads are not fresh tokens: the label must agree with the turn footer.
      tokens: { input: 4000, output: 100, cache_read: 90000, cache_write: 0 },
    }),
  );
  const { log } = mount([], { turns: [...turns].reverse() });
  const toggle = within(log).getByRole("button", {
    name: "Turn 1 · 0 files changed · 4.1k tokens",
  });
  expect(within(log).queryByText("Reply 1")).toBeNull();
  expect(log.querySelectorAll(".md")).toHaveLength(20);
  fireEvent.click(toggle);
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(within(log).getByText("Reply 1")).toBeTruthy();
  expect(Array.from(log.querySelectorAll(".md")).map((node) => node.textContent)).toEqual(
    turns.map((_, index) => `Reply ${index + 1}`),
  );
  fireEvent.click(toggle);
  expect(within(log).queryByText("Reply 1")).toBeNull();
});

it("pill when scrolled up, follow at bottom", () => {
  const first = turn([item("text", { id: "first", markdown: "First reply" })]);
  const h = mount([], { turns: [first] });
  Object.defineProperties(h.log, {
    scrollHeight: { configurable: true, value: 1000 },
    clientHeight: { configurable: true, value: 200 },
  });
  h.log.scrollTop = 0;
  fireEvent.scroll(h.log);
  const second = {
    ...first,
    items: [...first.items, item("text", { id: "second", markdown: "Second reply" })],
  };
  h.update([second]);
  expect(h.log.scrollTop).toBe(0);
  fireEvent.click(screen.getByRole("button", { name: "New activity" }));
  expect(h.log.scrollTop).toBe(1000);
  expect(screen.queryByRole("button", { name: "New activity" })).toBeNull();
  h.log.scrollTop = 800;
  fireEvent.scroll(h.log);
  Object.defineProperty(h.log, "scrollHeight", { configurable: true, value: 1200 });
  h.update([
    { ...second, items: [...second.items, item("text", { id: "third", markdown: "Third reply" })] },
  ]);
  expect(h.log.scrollTop).toBe(1200);
  expect(screen.queryByRole("button", { name: "New activity" })).toBeNull();
});

it("log region announces text blocks only", () => {
  const h = mount([
    item("tool", { summary: "Reading a file", status: "pending" }),
    item("text", { markdown: "Assistant response" }),
    item("user", { origin: null, attachments: [] }),
  ]);
  expect(h.log.getAttribute("aria-live")).toBe("polite");
  expect(h.log.getAttribute("aria-relevant")).toBe("additions");
  const reply = within(h.log).getByText("Assistant response");
  const block = reply.closest('[aria-atomic="true"]');
  expect(block).not.toBeNull();
  expect(block?.closest('[aria-live="off"]')).toBeNull();
  expect(within(h.log).getByText("Reading a file").closest('[aria-live="off"]')).not.toBeNull();
  const original = block;
  h.update([
    {
      ...h.store.get(snapshotAtom).turns[0]!,
      tokens: { input: 2, cache_write: 0, cache_read: 0, output: 3 },
    },
  ]);
  expect(within(h.log).getByText("Assistant response").closest('[aria-atomic="true"]')).toBe(
    original,
  );
});

it("relative and doc-prefixed links in a reply resolve against doc/", () => {
  const { log, router } = mount([
    item("text", {
      markdown: "[Relative](product/section_02.md) and [Prefixed](doc/product/section_02.md)",
    }),
  ]);
  const relative = within(log).getByRole("link", { name: "Relative" });
  const prefixed = within(log).getByRole("link", { name: "Prefixed" });
  expect(relative.getAttribute("href")).toBe("/files/product/section_02.md");
  expect(prefixed.getAttribute("href")).toBe("/files/product/section_02.md");
  fireEvent.click(prefixed);
  expect(router.state.location.pathname).toBe("/files/product/section_02.md");
});

it("reply headings carry no id", () => {
  const { log } = mount([item("text", { markdown: "# Heading\n\n## Other heading" })]);
  expect(
    within(log)
      .getAllByRole("heading")
      .map((heading) => heading.getAttribute("id")),
  ).toEqual([null, null]);
  expect(within(log).queryByRole("button", { name: /copy link to this heading/i })).toBeNull();
});

it("tool input with U+202E shows the marker", () => {
  const { log } = mount([
    item("tool", { summary: "Inspect input", input: "safe\u202Eevil", status: "done" }),
  ]);
  fireEvent.click(within(log).getByText("Inspect input"));
  expect(within(log).getByText("safe⟨U+202E⟩evil")).toBeTruthy();
  expect(log.textContent).not.toContain("\u202E");
});

it("previous chat precedes the divider and current turns", () => {
  const previous = {
    session: fixture.previous!.session,
    turns: [turn([item("text", { markdown: "Previous reply" })], { open: false })],
  };
  const { log } = mount([item("text", { markdown: "Current reply" })], { previous });
  expect(log.textContent).toMatch(/Previous reply[\s\S]*Chat ended at[\s\S]*Current reply/);
  expect(within(log).getByText("Previous reply").closest('[aria-live="off"]')).not.toBeNull();
});

it("ended tools fold while errors remain visible", () => {
  const tools = ["doc/a.md", "doc/b.md", "doc/a.md"].map((path, index) =>
    item("tool", {
      id: `tool-${index}`,
      name: "Edit",
      summary: `Edit ${index}`,
      status: "done",
      input: JSON.stringify({ file_path: path }),
      subagent: null,
      parent: null,
    }),
  );
  const { log } = mount([], {
    turns: [
      turn(
        [
          ...tools,
          item("tool", {
            id: "error",
            status: "error",
            summary: "Missing file",
            error_line: "File not found",
          }),
        ],
        { open: false },
      ),
    ],
  });
  const summary = within(log).getByText("Used 3 tools · edited 2 files");
  expect(summary.closest("details")?.open).toBe(false);
  // jsdom does not hide the children of a closed <details>, so assert containment instead.
  const folded = within(log).getByRole("button", { name: /Edit 0/ });
  expect(folded.closest("details")).toBe(summary.closest("details"));
  expect(within(log).getByText("File not found")).toBeTruthy();
  fireEvent.click(summary);
  expect(summary.closest("details")?.open).toBe(true);
});

it("subagent rows render once under their Agent", () => {
  const parent = item("tool", {
    id: "agent",
    name: "Agent",
    tool_use_id: "parent",
    summary: "Investigate",
    status: "done",
  });
  const nested = item("tool", {
    id: "child",
    subagent: "sub1",
    parent: "parent",
    summary: "Nested read",
    status: "done",
  });
  const { log } = mount([parent, nested]);
  expect(within(log).queryByText("Nested read")).toBeNull();
  fireEvent.click(within(log).getByText("Investigate"));
  expect(within(log).getAllByText("Nested read")).toHaveLength(1);
});

it("rewritten literal autolink keeps its full address", () => {
  const { log } = mount([item("text", { markdown: "Write to a@b.com today" })]);
  expect(log.textContent).toContain("Write to a@b.com today");
});

it("session divider shows the end time, or no time when it is unparsable", () => {
  const session = fixture.previous!.session;
  const previous = (state_since: string) => ({
    session: { ...session, state_since },
    turns: [turn([item("text", { markdown: "Previous reply" })], { open: false })],
  });
  const first = mount([], { previous: previous("2026-03-01T14:03:22Z") });
  expect(within(first.log).getByText(/^Chat ended at \d{2}:\d{2}$/)).toBeTruthy();
  cleanup();
  const second = mount([], { previous: previous("not a time") });
  expect(within(second.log).getByText("Chat ended")).toBeTruthy();
  expect(second.log.textContent).not.toMatch(/NaN/);
});
