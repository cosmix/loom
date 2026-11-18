import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { chatSnapshotSchema, type Item } from "@/plan/api/schema-chat";
import snapshot from "@/plan/api/fixtures/chat-snapshot.json";
import { PatchView } from "./patch-view";
import { ToolRow } from "./tool-row";

type ToolItem = Extract<Item, { type: "tool" }>;
const fixture = chatSnapshotSchema
  .parse(snapshot)
  .turns.flatMap((turn) => turn.items)
  .find((item): item is ToolItem => item.type === "tool")!;

function tool(overrides: Partial<ToolItem> = {}): ToolItem {
  return { ...fixture, status: "done", ...overrides };
}

afterEach(cleanup);

describe("ToolRow status", () => {
  it("renders summary as given", () => {
    render(<ToolRow item={tool({ summary: "Bridge wording: +12 −3 at doc/file.md" })} />);

    expect(screen.getByText("Bridge wording: +12 −3 at doc/file.md").textContent).toBe(
      "Bridge wording: +12 −3 at doc/file.md",
    );
    expect(screen.getByRole("button").getAttribute("aria-expanded")).toBe("false");
  });

  it("pending says Pending and draws no roundel: the pane has one, on the turn", () => {
    const { container } = render(<ToolRow item={tool({ status: "pending" })} />);

    expect(container.querySelector(".aurora-roundel")).toBeNull();
    expect(screen.getByText("Pending")).toBeTruthy();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("done without counts shows the word and its check", () => {
    const { container } = render(<ToolRow item={tool({ added: null, removed: null })} />);

    expect(screen.getByText("Done").querySelector("svg")).toBeTruthy();
    expect(container.querySelector(".aurora-roundel")).toBeNull();
  });

  it("denied shows note", () => {
    render(
      <ToolRow item={tool({ status: "denied", result: "Please leave doc/private.md alone." })} />,
    );

    expect(screen.getByText("Denied")).toBeTruthy();
    expect(screen.getByText("Please leave doc/private.md alone.")).toBeTruthy();
    expect(screen.getByRole("button").getAttribute("aria-expanded")).toBe("false");
  });

  it("not_finished label", () => {
    render(<ToolRow item={tool({ status: "not_finished" })} />);

    expect(screen.getByText("Not finished")).toBeTruthy();
  });

  it("a finished edit ends its row with toned counts instead of the status word", () => {
    render(<ToolRow item={tool({ name: "Edit", summary: "doc/a.md", added: 14, removed: 6 })} />);

    const row = screen.getByRole("button");
    expect(row.getAttribute("data-status")).toBe("done");
    expect(within(row).getByText("+14").className).toBe("chat-count-added");
    expect(within(row).getByText("−6").className).toBe("chat-count-removed");
    expect(screen.queryByText("Done")).toBeNull();
  });

  it("each row leads with one glyph in its column and the target in mono", () => {
    render(<ToolRow item={tool({ name: "Bash", summary: "Run ls" })} />);

    const row = screen.getByRole("button", { name: /^Bash Run ls/ });
    expect(row.querySelectorAll(".chat-tool-glyph svg")).toHaveLength(1);
    expect(screen.getByText("Run ls").closest(".chat-tool-target")).not.toBeNull();
    expect(row.querySelector(".chat-tool-name, .chat-tool-chevron")).toBeNull();
  });
});

describe("ToolRow expanded output", () => {
  it("result capped at 40 lines with Show all", () => {
    const result = Array.from({ length: 42 }, (_, index) => `output line ${index + 1}`).join("\n");
    const { container } = render(<ToolRow item={tool({ result })} />);
    fireEvent.click(screen.getByRole("button"));

    const output = container.querySelectorAll("pre")[1];
    expect(output.textContent?.split("\n")).toHaveLength(40);
    expect(output.textContent).toContain("output line 40");
    expect(output.textContent).not.toContain("output line 41");

    fireEvent.click(screen.getByRole("button", { name: "Show all" }));
    expect(output.textContent).toBe(result);
    expect(screen.queryByRole("button", { name: "Show all" })).toBeNull();
  });

  it("edit expands to coloured patch", () => {
    const patch = "--- a/doc/file.md\n+++ b/doc/file.md\n@@ -1 +1 @@\n-old\n+new\n";
    const { container } = render(<ToolRow item={tool({ name: "Edit", patch })} />);
    expect(screen.queryByText("+new")).toBeNull();

    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("+new").parentElement?.style.color).toBe("var(--tone-completed)");
    expect(screen.getByText("-old").parentElement?.style.color).toBe("var(--tone-blocked)");
    expect(screen.getByText("+++ b/doc/file.md").parentElement?.style.color).toBe("");
    expect(
      Array.from(container.querySelectorAll("pre")).some((pre) => pre.textContent === patch),
    ).toBe(true);
  });

  it("write expands to a patch", () => {
    render(<ToolRow item={tool({ name: "Write", patch: "@@ -0,0 +1 @@\n+written" })} />);

    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("+written").parentElement?.style.color).toBe("var(--tone-completed)");
  });
});

describe("ToolRow agent rows", () => {
  it("agent row nests subagent items collapsed and shows result", () => {
    const child = tool({
      id: "child",
      tool_use_id: "nested-use",
      parent: "agent-use",
      subagent: "agent-1",
      summary: "Nested read",
      input: "nested input",
      result: "nested result",
    });
    const unrelated = tool({ id: "other", parent: "other-agent", summary: "Unrelated read" });
    const { container } = render(
      <ToolRow
        item={tool({
          name: "Agent",
          tool_use_id: "agent-use",
          nested_tools: 3,
          result: "Final agent answer",
        })}
        nested={[child, unrelated]}
      />,
    );
    expect(screen.getByText("3 tool calls")).toBeTruthy();
    expect(screen.queryByText("Final agent answer")).toBeNull();

    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("Final agent answer")).toBeTruthy();
    const nestedButton = screen.getByRole("button", { name: /Nested read/ });
    expect(nestedButton.getAttribute("aria-expanded")).toBe("false");
    expect(nestedButton.parentElement?.parentElement?.className).toContain("pl-4");
    expect(screen.queryByText("nested result")).toBeNull();
    expect(screen.queryByText("Unrelated read")).toBeNull();

    fireEvent.click(nestedButton);
    expect(within(container).getByText("nested result")).toBeTruthy();
  });

  it("running agent shows its count and no roundel", () => {
    const { container } = render(
      <ToolRow item={tool({ name: "Agent", status: "pending", nested_tools: 1 })} />,
    );

    expect(screen.getByText("1 tool call")).toBeTruthy();
    expect(container.querySelector(".aurora-roundel")).toBeNull();
  });
});

describe("ToolRow errors and text safety", () => {
  it("html input and result render as text", () => {
    const { container } = render(
      <ToolRow
        item={tool({ input: '{ "command": "<b>rm</b>" }', result: "<script>x()</script>" })}
      />,
    );
    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText(/<b>rm<\/b>/).closest("pre")).toBeTruthy();
    expect(screen.getByText("<script>x()</script>").closest("pre")).toBeTruthy();
    expect(container.querySelector("b, script")).toBeNull();
  });

  it("first error line shown without expanding", () => {
    render(
      <ToolRow
        item={tool({
          status: "error",
          error_line: "Permission denied: doc/locked.md\ninternal details",
          result: "Permission denied: doc/locked.md\nwhile running the hook",
        })}
      />,
    );

    expect(screen.getByText("Permission denied: doc/locked.md")).toBeTruthy();
    expect(screen.queryByText(/while running the hook|internal details/)).toBeNull();
    expect(screen.getByRole("button").getAttribute("aria-expanded")).toBe("false");
  });

  it("falls back to the first result line for errors", () => {
    render(
      <ToolRow
        item={tool({ status: "error", error_line: null, result: "Failed to read\nstack trace" })}
      />,
    );

    expect(screen.getByText("Failed to read")).toBeTruthy();
    expect(screen.queryByText(/stack trace/)).toBeNull();
  });
});

describe("ToolRow format controls", () => {
  it("tool input with U+202E shows the marker", () => {
    const { container } = render(
      <ToolRow item={tool({ input: '{ "path": "doc/\u202esecret.md" }' })} />,
    );
    fireEvent.click(screen.getByRole("button"));

    const input = screen.getByText(/doc\/⟨U\+202E⟩secret.md/);
    expect(input.style.unicodeBidi).toBe("isolate");
    expect(input.style.direction).toBe("ltr");
    expect(container.textContent).not.toContain("\u202e");
  });

  it("summary, result and error paths show format controls visibly", () => {
    const { container } = render(
      <ToolRow
        item={tool({
          status: "error",
          summary: "doc/\u200bfile.md",
          error_line: "doc/\u2066locked.md",
          result: "result\u202e",
        })}
      />,
    );
    expect(screen.getByText("doc/⟨U+200B⟩file.md")).toBeTruthy();
    expect(screen.getByText("doc/⟨U+2066⟩locked.md")).toBeTruthy();

    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("result⟨U+202E⟩")).toBeTruthy();
    expect(container.textContent).not.toMatch(/\p{Cf}/u);
  });

  it("bridge truncation remains visible after showing all available lines", () => {
    render(<ToolRow item={tool({ result: "Available result", result_truncated: true })} />);
    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("Result truncated.")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Show all" })).toBeNull();
  });
});

describe("PatchView", () => {
  it("colours repeated plus and minus prefixes inside hunks as content", () => {
    render(
      <PatchView patch={"--- a/doc/file.md\n+++ b/doc/file.md\n@@ -1 +1 @@\n--- old\n+++ new\n"} />,
    );

    expect(screen.getByText("--- old").parentElement?.style.color).toBe("var(--tone-blocked)");
    expect(screen.getByText("+++ new").parentElement?.style.color).toBe("var(--tone-completed)");
  });

  it("patch paths and lines render through Untrusted", () => {
    const { container } = render(
      <PatchView patch={"+++ b/doc/\u202efile.md\n+<img src=x>\u200b\n"} />,
    );

    expect(container.querySelector("pre")?.textContent).toBe(
      "+++ b/doc/⟨U+202E⟩file.md\n+<img src=x>⟨U+200B⟩\n",
    );
    expect(container.querySelector("img")).toBeNull();
  });
});
