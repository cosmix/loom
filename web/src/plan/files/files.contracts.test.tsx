import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { createStore } from "jotai";
import { Provider } from "jotai/react";
import type { ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { afterEach, describe, expect, it } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import treeFixture from "@/plan/api/fixtures/files-tree.json";
import { fileTreeSchema, type SectionView } from "@/plan/api/schema-files";
import { AddToChat, registerSelectionSecondary, type AttachmentDraft } from "@/plan/attachments";
import { renderedViewProvenance } from "@/plan/attachments/view-provenance";
import { Tree } from "@/plan/files/tree";
import { Markdown } from "@/plan/markdown";
import { authoringAtom, canAuthorAtom, serverInfoAtom, youAtom } from "@/plan/state/atoms";

type Store = ReturnType<typeof createStore>;

function renderIn(store: Store, node: ReactNode) {
  return render(
    <Provider store={store}>
      <MemoryRouter initialEntries={["/files"]}>
        <TooltipProvider>{node}</TooltipProvider>
      </MemoryRouter>
    </Provider>,
  );
}

function firstExact(label: string): HTMLElement {
  const [element] = screen.getAllByText(label, { exact: true });
  return element;
}

function precedes(a: Node, b: Node): boolean {
  return (a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
}

afterEach(() => {
  window.getSelection()?.removeAllRanges();
  cleanup();
});

describe("file mode contracts", () => {
  it("file tree orders product plans knowledge and other then archive", () => {
    const tree = fileTreeSchema.parse(treeFixture);
    renderIn(createStore(), <Tree tree={tree} />);

    const groups = ["Product", "Plans", "Knowledge", "Other", "Archive"].map(firstExact);
    for (let index = 1; index < groups.length; index += 1) {
      expect(precedes(groups[index - 1], groups[index])).toBe(true);
    }

    // The gap entry stays visible on the Product tab, marked "missing" inside its own entry.
    fireEvent.click(screen.getByRole("button", { name: "Product" }));
    let entry: HTMLElement | null = firstExact("Section 2");
    while (entry && !/missing/i.test(entry.textContent ?? "")) {
      entry = entry.parentElement;
    }
    expect(entry).not.toBeNull();
    expect(entry?.textContent ?? "").not.toContain("Vision");
  });

  it("a read only browser sees the registered secondary instead of add to chat", () => {
    const store = createStore();
    store.set(serverInfoAtom, {
      project: "example",
      project_key: "example-1234",
      port: 7272,
      remote: false,
      read_only: [],
      stopping: false,
      you: { public_id: "aaaaaaaaaaaaaaaa", name: "Reader" },
    });
    store.set(youAtom, { public_id: "aaaaaaaaaaaaaaaa", name: "Reader" });
    store.set(authoringAtom, {
      holder: { public_id: "bbbbbbbbbbbbbbbb", name: "Ada", away_since: null, frees_at: null },
    });
    expect(store.get(canAuthorAtom)).toBe(false);

    const draft: AttachmentDraft = {
      kind: "source",
      path: "doc/notes.md",
      anchor: "purpose",
      text: "Selected words",
      lines: null,
      stageId: null,
      hash: "abc123",
    };
    const rect = {
      x: 10,
      y: 10,
      width: 100,
      height: 20,
      top: 10,
      left: 10,
      right: 110,
      bottom: 30,
    };
    const unregister = registerSelectionSecondary((given) => (
      <button type="button">Send to author: {given.text}</button>
    ));
    try {
      renderIn(store, <AddToChat draft={draft} rect={rect as DOMRect} onAdded={() => {}} />);
      expect(screen.getByRole("button", { name: /Send to author: Selected words/ })).toBeTruthy();
      expect(screen.queryByRole("button", { name: /add to chat/i })).toBeNull();
    } finally {
      unregister();
    }
  });

  it("add to chat records the anchor of the section holding the selection", () => {
    const source = "## First\n\nAlpha words here.\n\n## Second\n\nBeta words here.\n";
    const sections: SectionView[] = [
      { level: 2, title: "First", anchor: "first", start_line: 1, end_line: 4 },
      { level: 2, title: "Second", anchor: "second", start_line: 5, end_line: 7 },
    ];
    const path = "doc/notes.md";
    renderIn(
      createStore(),
      <Markdown
        source={source}
        path={path}
        sections={sections}
        onNavigate={() => {}}
        docPaths={new Set([path])}
        anchorsOf={() => new Set(["first", "second"])}
      />,
    );

    const textNode = screen.getByText("Beta words here.").firstChild;
    expect(textNode).not.toBeNull();
    const range = document.createRange();
    range.setStart(textNode as Node, 0);
    range.setEnd(textNode as Node, 4);
    const selection = window.getSelection() as Selection;
    selection.removeAllRanges();
    selection.addRange(range);

    const draft = renderedViewProvenance(path, sections)(selection);
    expect(draft).not.toBeNull();
    expect(draft).toMatchObject({ kind: "source", path, anchor: "second", text: "Beta" });
  });
});
