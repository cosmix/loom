import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { SourceEditor } from "./source-editor";

// A stand-in for the lazy chunk: a textarea that reports Ctrl-S the way the real
// keymap does, and records what the component asks of the handle.
const fake = vi.hoisted(() => {
  const marks: number[][] = [];
  const destroyed = vi.fn();
  const modes: string[] = [];
  const created: { mode?: string; onFollowLink?: (href: string) => void }[] = [];
  return {
    marks,
    destroyed,
    modes,
    created,
    createSourceEditor(
      parent: HTMLElement,
      options: {
        doc: string;
        mode?: "preview" | "source";
        onChange: (text: string) => void;
        onSave: () => void;
        onFollowLink?: (href: string) => void;
      },
    ) {
      created.push(options);
      let mode = options.mode ?? "preview";
      const element = document.createElement("textarea");
      element.dataset.testid = "source-editor";
      element.value = options.doc;
      element.addEventListener("input", () => options.onChange(element.value));
      element.addEventListener("keydown", (event) => {
        if (event.key === "s" && (event.ctrlKey || event.metaKey)) {
          event.preventDefault();
          options.onSave();
        }
      });
      parent.append(element);
      return {
        destroy: () => {
          destroyed();
          element.remove();
        },
        getText: () => element.value,
        setText: (text: string) => {
          element.value = text;
        },
        searchOpen: () => false,
        setGutterMarks: (lines: number[]) => marks.push(lines),
        mode: () => mode,
        setMode: (next: "preview" | "source") => {
          modes.push(next);
          mode = next;
        },
      };
    },
  };
});

vi.mock("./load", () => ({ loadCodeMirror: () => Promise.resolve(fake) }));

afterEach(() => {
  cleanup();
  fake.marks.length = 0;
  fake.modes.length = 0;
  fake.created.length = 0;
  fake.destroyed.mockClear();
});

function renderEditor(overrides: Partial<Parameters<typeof SourceEditor>[0]> = {}) {
  const props = {
    path: "product/a.md",
    text: "# A\n",
    mode: "preview" as const,
    onChange: vi.fn(),
    onSave: vi.fn(),
    ...overrides,
  };
  const view = render(<SourceEditor {...props} />);
  return { props, view };
}

describe("SourceEditor", () => {
  it("shows a loading line, then the editor with the document text", async () => {
    renderEditor();
    expect(screen.getByText("Loading editor")).toBeTruthy();
    const area = (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;
    expect(area.value).toBe("# A\n");
    expect(screen.queryByText("Loading editor")).toBeNull();
  });

  it("calls onSave on Ctrl-S and Cmd-S and prevents the browser save", async () => {
    const { props } = renderEditor();
    const area = await screen.findByTestId("source-editor");
    expect(fireEvent.keyDown(area, { key: "s", ctrlKey: true })).toBe(false);
    expect(fireEvent.keyDown(area, { key: "s", metaKey: true })).toBe(false);
    expect(props.onSave).toHaveBeenCalledTimes(2);
  });

  it("forwards edits to onChange", async () => {
    const { props } = renderEditor();
    const area = await screen.findByTestId("source-editor");
    fireEvent.input(area, { target: { value: "# B\n" } });
    expect(props.onChange).toHaveBeenCalledWith("# B\n");
  });

  it("pushes gutter marks and rebuilds the editor when the path changes", async () => {
    const { props, view } = renderEditor({ gutterMarks: [2, 5] });
    await screen.findByTestId("source-editor");
    await waitFor(() => expect(fake.marks.at(-1)).toEqual([2, 5]));

    view.rerender(<SourceEditor {...props} path="product/b.md" text={"# C\n"} />);
    const area = (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;
    expect(fake.destroyed).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(area.value).toBe("# C\n"));
  });

  it("keeps the same editor instance when only the text changes after a save", async () => {
    const { props, view } = renderEditor();
    const area = (await screen.findByTestId("source-editor")) as HTMLTextAreaElement;
    fireEvent.input(area, { target: { value: "# B\n" } });
    // A save moves the opened hash and keeps the text; a prop-only change must not rebuild.
    view.rerender(<SourceEditor {...props} text={"# B\n"} />);
    view.rerender(<SourceEditor {...props} text={"# B\n"} gutterMarks={[1]} />);
    expect(fake.destroyed).not.toHaveBeenCalled();
    expect(screen.getByTestId("source-editor")).toBe(area);
    expect(area.value).toBe("# B\n");
    expect(screen.queryByText("Loading editor")).toBeNull();
  });

  it("builds the editor in the mode prop and does not set it on mount", async () => {
    renderEditor({ mode: "source" });
    await screen.findByTestId("source-editor");
    expect(fake.created).toHaveLength(1);
    expect(fake.created[0]!.mode).toBe("source");
    expect(fake.modes).toEqual([]);
  });

  it("follows a change of the mode prop without rebuilding the editor", async () => {
    const { props, view } = renderEditor();
    const area = await screen.findByTestId("source-editor");
    view.rerender(<SourceEditor {...props} mode="source" />);
    await waitFor(() => expect(fake.modes).toEqual(["source"]));
    view.rerender(<SourceEditor {...props} mode="source" text={"# B\n"} />);
    expect(fake.modes).toEqual(["source"]);
    expect(fake.created).toHaveLength(1);
    expect(fake.destroyed).not.toHaveBeenCalled();
    expect(screen.getByTestId("source-editor")).toBe(area);
  });

  it("passes link follows to the latest onFollowLink prop", async () => {
    const first = vi.fn();
    const second = vi.fn();
    const { props, view } = renderEditor({ onFollowLink: first });
    await screen.findByTestId("source-editor");
    view.rerender(<SourceEditor {...props} onFollowLink={second} />);
    fake.created[0]!.onFollowLink!("a.md");
    expect(first).not.toHaveBeenCalled();
    expect(second).toHaveBeenCalledWith("a.md");
  });
});
