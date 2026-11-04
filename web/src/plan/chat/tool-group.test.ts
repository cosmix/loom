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

const group = (items: ToolItem[], label: string) => ({ kind: "group", items, label });

describe("groupToolRows folding", () => {
  it("folds a single tool row, so a running turn shows one quiet line", () => {
    const only = tool("1", { status: "pending" });

    expect(groupToolRows([only])).toEqual([group([only], "Used 1 tool · edited 0 files")]);
  });

  it("two rows fold", () => {
    const items = [tool("1"), tool("2")];

    expect(groupToolRows(items)).toEqual([group(items, "Used 2 tools · edited 0 files")]);
  });

  it.each(["error", "denied"] as const)("%s in the middle splits the run in two", (status) => {
    const before = [tool("1"), tool("2")];
    const stopped = tool("3", { status });
    const after = [tool("4"), tool("5")];

    expect(groupToolRows([...before, stopped, ...after])).toEqual([
      group(before, "Used 2 tools · edited 0 files"),
      stopped,
      group(after, "Used 2 tools · edited 0 files"),
    ]);
  });

  it("three rows fold with the right label", () => {
    const items = [
      tool("1", { name: "Edit", input: '{"file_path":"doc/alpha.md"}' }),
      tool("2", { name: "Write", input: '{"file_path":"doc/beta.md"}' }),
      tool("3"),
    ];

    expect(groupToolRows(items)).toEqual([group(items, "Used 3 tools · edited 2 files")]);
  });
});

describe("groupToolRows skill rows", () => {
  it("drops a Skill tool row before grouping, so it neither shows nor counts", () => {
    const skill = tool("skill", { name: "Skill", summary: "loom-plan:kickoff" });
    const rows = [tool("1"), tool("2")];

    expect(groupToolRows([skill, ...rows])).toEqual([group(rows, "Used 2 tools · edited 0 files")]);
    expect(groupToolRows([skill])).toEqual([]);
  });

  it("drops a Skill tool row that failed, and keeps the run on both sides whole", () => {
    const skill = tool("skill", { name: "Skill", status: "error" });
    const before = [tool("1")];
    const after = [tool("2")];

    expect(groupToolRows([...before, skill, ...after])).toEqual([
      group([...before, ...after], "Used 2 tools · edited 0 files"),
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

    expect(groupToolRows(items)).toEqual([group(items, "Used 3 tools · edited 1 files")]);
  });

  it.each(["{", "null", "{}", "[]", '{"file_path":7}', '{"file_path":""}'])(
    "ignores an unusable edited path in %s",
    (input) => {
      const items = [tool("1", { name: "Edit", input }), tool("2"), tool("3")];

      expect(groupToolRows(items)).toEqual([group(items, "Used 3 tools · edited 0 files")]);
    },
  );
});

describe("groupToolRows boundaries", () => {
  it("preserves text between independent groups", () => {
    const before = [tool("1"), tool("2"), tool("3")];
    const text: Item = { type: "text", id: "text", markdown: "Next step" };
    const after = [tool("4"), tool("5"), tool("6")];

    expect(groupToolRows([...before, text, ...after])).toEqual([
      group(before, "Used 3 tools · edited 0 files"),
      text,
      group(after, "Used 3 tools · edited 0 files"),
    ]);
  });
});

describe("groupToolRows nested and empty", () => {
  it("leaves nested subagent tools outside the runs on either side", () => {
    const agent = tool("agent", { name: "Agent", nested_tools: 1 });
    const nested = tool("nested", { subagent: "child", parent: agent.tool_use_id });
    const rest = [tool("1"), tool("2")];

    expect(groupToolRows([agent, nested, ...rest])).toEqual([
      group([agent], "Used 1 tool · edited 0 files"),
      nested,
      group(rest, "Used 2 tools · edited 0 files"),
    ]);
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

    expect(groupToolRows([agent, nested, ...items])).toEqual([
      group([agent], "Used 1 tool · edited 0 files"),
      nested,
      group(items, "Used 3 tools · edited 0 files"),
    ]);
  });

  it("preserves an empty transcript", () => {
    expect(groupToolRows([])).toEqual([]);
  });
});
