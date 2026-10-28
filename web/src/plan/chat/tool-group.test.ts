import { describe, expect, it } from "vitest";

import type { Item } from "@/plan/api/schema-chat";
import { groupToolRows } from "./tool-group";

type ToolItem = Extract<Item, { type: "tool" }>;

function tool(id: string, overrides: Partial<ToolItem> = {}): ToolItem {
  return {
    type: "tool",
    id,
    tool_use_id: `use-${id}`,
    name: "Read",
    summary: "Read a file",
    status: "done",
    subagent: null,
    parent: null,
    input: "{}",
    result: null,
    result_truncated: false,
    error_line: null,
    added: null,
    removed: null,
    patch: null,
    nested_tools: 0,
    ...overrides,
  };
}

describe("groupToolRows folding", () => {
  it("two rows no fold", () => {
    const items = [tool("1"), tool("2")];

    expect(groupToolRows(items, true)).toEqual(items);
  });

  it("running turn no fold", () => {
    const items = [tool("1"), tool("2"), tool("3")];

    expect(groupToolRows(items, false)).toEqual(items);
  });

  it.each(["error", "denied"] as const)("%s in the middle breaks the run", (status) => {
    const items = [tool("1"), tool("2"), tool("3", { status }), tool("4"), tool("5")];

    expect(groupToolRows(items, true)).toEqual(items);
  });

  it("three rows fold with the right label", () => {
    const items = [
      tool("1", { name: "Edit", input: '{"file_path":"doc/alpha.md"}' }),
      tool("2", { name: "Write", input: '{"file_path":"doc/beta.md"}' }),
      tool("3"),
    ];

    expect(groupToolRows(items, true)).toEqual([
      { kind: "group", items, label: "Used 3 tools · edited 2 files" },
    ]);
  });
});

describe("groupToolRows labels", () => {
  it("counts repeated Edit and Write paths once and excludes Read paths", () => {
    const items = [
      tool("1", { name: "Edit", input: '{"file_path":"doc/alpha.md"}' }),
      tool("2", { name: "Write", input: '{"file_path":"doc/alpha.md"}' }),
      tool("3", { input: '{"file_path":"doc/beta.md"}' }),
    ];

    expect(groupToolRows(items, true)).toEqual([
      { kind: "group", items, label: "Used 3 tools · edited 1 files" },
    ]);
  });

  it.each(["{", "null", "{}", "[]", '{"file_path":7}', '{"file_path":""}'])(
    "ignores an unusable edited path in %s",
    (input) => {
      const items = [tool("1", { name: "Edit", input }), tool("2"), tool("3")];

      expect(groupToolRows(items, true)).toEqual([
        { kind: "group", items, label: "Used 3 tools · edited 0 files" },
      ]);
    },
  );
});

describe("groupToolRows boundaries", () => {
  it("preserves text between independent groups", () => {
    const before = [tool("1"), tool("2"), tool("3")];
    const text: Item = { type: "text", id: "text", markdown: "Next step" };
    const after = [tool("4"), tool("5"), tool("6")];

    expect(groupToolRows([...before, text, ...after], true)).toEqual([
      { kind: "group", items: before, label: "Used 3 tools · edited 0 files" },
      text,
      { kind: "group", items: after, label: "Used 3 tools · edited 0 files" },
    ]);
  });

  it.each(["error", "denied"] as const)("keeps a %s row outside adjacent groups", (status) => {
    const before = [tool("1"), tool("2"), tool("3")];
    const failed = tool("failed", { status });
    const after = [tool("4"), tool("5"), tool("6")];

    expect(groupToolRows([...before, failed, ...after], true)).toEqual([
      { kind: "group", items: before, label: "Used 3 tools · edited 0 files" },
      failed,
      { kind: "group", items: after, label: "Used 3 tools · edited 0 files" },
    ]);
  });
});

describe("groupToolRows nested and empty", () => {
  it("leaves nested subagent tools attached to their Agent row outside the run", () => {
    const agent = tool("agent", { name: "Agent", nested_tools: 1 });
    const nested = tool("nested", { subagent: "child", parent: agent.tool_use_id });
    const items = [agent, nested, tool("1"), tool("2")];

    expect(groupToolRows(items, true)).toEqual(items);
  });

  it("preserves nested tools and excludes their edited paths from a following group", () => {
    const agent = tool("agent", { name: "Agent", nested_tools: 1 });
    const nested = tool("nested", {
      subagent: "child",
      parent: agent.tool_use_id,
      name: "Edit",
      input: '{"file_path":"doc/nested.md"}',
    });
    const items = [tool("1"), tool("2"), tool("3")];

    expect(groupToolRows([agent, nested, ...items], true)).toEqual([
      agent,
      nested,
      { kind: "group", items, label: "Used 3 tools · edited 0 files" },
    ]);
  });

  it("preserves an empty transcript", () => {
    expect(groupToolRows([], true)).toEqual([]);
  });
});
