import { cleanup, fireEvent, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { SectionView } from "@/plan/api/schema-files";
import {
  collectDefinitions,
  headingAnchorLookup,
  type RenderContext,
} from "@/plan/markdown/context";
import gfmSource from "@/plan/markdown/fixtures/gfm.md?raw";
import { parseMarkdown } from "@/plan/markdown/markdown";
import { renderBlocks } from "@/plan/markdown/render";
import {
  expectInert,
  linkNamed,
  PATH,
  renderMarkdown,
  section,
} from "@/plan/markdown/render-test-utils";

const writeText = vi.fn<(text: string) => Promise<void>>();

beforeEach(() => {
  writeText.mockReset().mockResolvedValue(undefined);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
});

afterEach(() => {
  cleanup();
  Reflect.deleteProperty(navigator, "clipboard");
});

function renderWithTitle(
  source: string,
  suppressTitle?: string | null,
  sections: SectionView[] = [],
): ReturnType<typeof render> {
  const root = parseMarkdown(source);
  const context: RenderContext = {
    path: PATH,
    onNavigate: vi.fn(),
    docPaths: new Set([PATH]),
    anchorsOf: () => null,
    headingAnchor: headingAnchorLookup(sections, 0),
    headingIds: true,
    suppressTitle,
    marks: null,
    definitions: collectDefinitions(root),
  };
  return render(<div className="md">{renderBlocks(root.children, context)}</div>);
}

describe("Markdown GFM rendering", () => {
  it("renders a table, task list, strikethrough, autolink and code blocks", () => {
    const { container } = renderMarkdown(gfmSource);
    expectInert(container);

    const table = container.querySelector(".md-table-scroll > table");
    expect(table?.querySelectorAll("thead th")).toHaveLength(2);
    expect(table?.querySelectorAll("tbody tr")).toHaveLength(2);
    expect(table?.querySelector("thead th:last-child")?.className).toBe("md-align-right");

    const boxes = Array.from(container.querySelectorAll<HTMLInputElement>("input[type=checkbox]"));
    expect(boxes.map((box) => [box.checked, box.disabled])).toEqual([
      [true, true],
      [false, true],
    ]);
    expect(container.querySelector("del")?.textContent).toBe("gone");
    expect(linkNamed(container, "https://example.com")?.getAttribute("rel")).toBe(
      "noopener noreferrer",
    );

    const blocks = Array.from(container.querySelectorAll(".md-code"));
    expect(blocks.map((block) => block.querySelector(".md-code-lang")?.textContent)).toEqual([
      "rust",
      "mermaid",
    ]);
    expect(blocks[0]?.querySelector("pre > code")?.textContent).toBe("fn main() {}");
    expect(blocks[1]?.querySelector("pre > code")?.textContent).toContain("graph TD");
    expect(container.querySelector("svg.mermaid, [data-mermaid]")).toBeNull();
  });

  it("copies a code block's text", async () => {
    const { container } = renderMarkdown(gfmSource);
    const copy = container.querySelector<HTMLButtonElement>(".md-code button");
    expect(copy?.getAttribute("aria-label")).toBe("Copy rust code");
    fireEvent.click(copy!);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("fn main() {}"));
  });

  it("highlights whole marked terms, ignoring case", () => {
    const { container } = renderMarkdown("Plan the planning; PLAN again.\n", { marks: ["plan"] });
    const marks = Array.from(container.querySelectorAll("mark")).map((mark) => mark.textContent);
    expect(marks).toEqual(["Plan", "PLAN"]);
    expect(container.textContent).toBe("Plan the planning; PLAN again.");
  });
});

describe("Markdown headings", () => {
  it("takes the server anchor and copies the heading address", async () => {
    const { container } = renderMarkdown("# Title\n\ntext\n\n## Intro\n", {
      sections: [section("title", 1, 1), section("intro-x", 5)],
    });
    expect(container.querySelector("h1")?.id).toBe("title");
    expect(container.querySelector("h2")?.id).toBe("intro-x");
    const copy = container.querySelector<HTMLButtonElement>("h2 + button");
    expect(copy?.getAttribute("aria-label")).toBe("Copy link to this heading");
    fireEvent.click(copy!);
    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith(`${window.location.origin}/files/notes.md#intro-x`),
    );
  });

  it("an excerpt maps its lines through lineOffset", () => {
    const { container } = renderMarkdown("## Pressure tests\n\nrecord\n", {
      sections: [section("other", 1), section("pressure-tests", 290)],
      lineOffset: 289,
    });
    expect(container.querySelector("h2")?.id).toBe("pressure-tests");
  });

  it("a heading with no section on its line has no id", () => {
    const { container } = renderMarkdown("## Loose\n", { sections: [section("x", 9)] });
    expect(container.querySelector("[id]")).toBeNull();
    expect(container.querySelector("button")).toBeNull();
  });

  it("headingIds false leaves no element id in repeated renders", () => {
    const source = "# Title\n\n- [ ] task\n\nNote[^1]\n\n[^1]: foot\n";
    const props = { sections: [section("title", 1, 1)], headingIds: false };
    const first = renderMarkdown(source, props).container;
    const second = renderMarkdown(source, props).container;
    expect(first.querySelector("h1")).not.toBeNull();
    expect(document.body.querySelectorAll("[id]")).toHaveLength(0);
    expect(second.querySelector("button")).toBeNull();
  });
});

describe("Markdown title suppression", () => {
  it("drops the first matching top-level H1 and keeps the body", () => {
    const { container } = renderWithTitle("# Title\n\nBody text.\n", "Title");
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector("p")?.textContent).toBe("Body text.");
  });

  it("finds the first H1 after other top-level blocks", () => {
    const { container } = renderWithTitle("Intro.\n\n## Title\n\n# Title\n", "Title");
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector("h2")?.textContent).toBe("Title");
    expect(container.querySelector("p")?.textContent).toBe("Intro.");
  });

  it.each(["Different", "title"])("keeps a different first H1 (%s) and a later match", (first) => {
    const { container } = renderWithTitle(`# ${first}\n\n# Title\n`, "Title");
    expect(Array.from(container.querySelectorAll("h1"), (heading) => heading.textContent)).toEqual([
      first,
      "Title",
    ]);
  });

  it("keeps a second matching H1 after suppressing the first", () => {
    const { container } = renderWithTitle("# Title\n\n# Title\n", "Title");
    expect(container.querySelectorAll("h1")).toHaveLength(1);
    expect(container.querySelector("h1")?.textContent).toBe("Title");
  });

  it("compares normalized whitespace and recursively unwrapped inline text", () => {
    const { container } = renderWithTitle(
      "#   *Execution **handoff***  [for `docs`](https://example.com)\t \n",
      " \tExecution  handoff\nfor docs  ",
    );
    expect(container.querySelector("h1")).toBeNull();
  });

  it.each(["> # Title", "- # Title"])("keeps nested headings in %s", (nested) => {
    const { container } = renderWithTitle(`${nested}\n\n# Title\n\n${nested}\n`, "Title");
    expect(container.querySelectorAll("h1")).toHaveLength(2);
    expect(container.querySelectorAll("blockquote h1, li h1")).toHaveLength(2);
  });

  it.each([undefined, null, "", " \t\n "])("keeps headings without a title (%s)", (title) => {
    const { container } = renderWithTitle("# Title\n\n# Title\n", title);
    expect(container.querySelectorAll("h1")).toHaveLength(2);
  });

  it("preserves the server anchors of later headings", () => {
    const source = "# Title\n\n## Details\n\n# Appendix\n\n### End\n";
    const sections = [
      section("title", 1, 1),
      section("details-server", 3),
      section("appendix-server", 5, 1),
      section("end-server", 7, 3),
    ];
    const original = renderWithTitle(source, null, sections).container;
    const suppressed = renderWithTitle(source, "Title", sections).container;
    const laterIds = Array.from(
      original.querySelectorAll("h1, h2, h3"),
      (heading) => heading.id,
    ).slice(1);
    expect(laterIds).toEqual(["details-server", "appendix-server", "end-server"]);
    expect(Array.from(suppressed.querySelectorAll("h1, h2, h3"), (heading) => heading.id)).toEqual(
      laterIds,
    );
  });

  it("leaves an empty anchor target with the suppressed title's server id", () => {
    const sections = [section("title-server", 1, 1)];
    const { container } = renderWithTitle("# Title\n\nBody.\n", "Title", sections);
    const target = container.querySelector("#title-server");
    expect(container.querySelector("h1")).toBeNull();
    expect(target).not.toBeNull();
    expect(target?.textContent).toBe("");
    expect(target?.children).toHaveLength(0);
  });

  it("leaves no anchor target when the suppressed title has no server anchor", () => {
    const { container } = renderWithTitle("# Title\n\nBody.\n", "Title");
    expect(container.querySelectorAll("[id]")).toHaveLength(0);
  });
});

describe("Markdown links", () => {
  it("resolves links relative to the document path", () => {
    const docs = new Set(["doc/product/section_03.md", "doc/plans/PLAN-x.md"]);
    const { container, onNavigate } = renderMarkdown(
      "[p](section_03.md#port) [q](../plans/PLAN-x.md) [r](../../README.md)\n",
      { path: "doc/product/section_02.md", docPaths: docs },
    );
    const p = linkNamed(container, "p");
    expect(p?.getAttribute("href")).toBe("/files/product/section_03.md#port");
    expect(linkNamed(container, "q")?.getAttribute("href")).toBe("/files/plans/PLAN-x.md");
    expect(linkNamed(container, "r")).toBeUndefined();
    expect(container.querySelector(".md-outside")?.getAttribute("title")).toBe("/README.md");
    fireEvent.click(p!);
    expect(onNavigate).toHaveBeenCalledWith("/files/product/section_03.md#port");
  });

  it("strikes a link to a known document's missing anchor", () => {
    const { container } = renderMarkdown("[a](notes.md#nope) [b](notes.md#yes)\n", {
      anchorsOf: () => new Set(["yes"]),
    });
    expect(linkNamed(container, "a")?.getAttribute("title")).toBe("not found");
    expect(linkNamed(container, "b")?.hasAttribute("title")).toBe(false);
  });

  it("an anchor link scrolls within the document", () => {
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    const { container, onNavigate } = renderMarkdown("## Intro\n\n[jump](#intro)\n", {
      sections: [section("intro", 1)],
    });
    fireEvent.click(linkNamed(container, "jump")!);
    Reflect.deleteProperty(Element.prototype, "scrollIntoView");
    expect(scroll).toHaveBeenCalledTimes(1);
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it("inline code naming a listed document links to it", () => {
    const listed = "doc/" + "plans/briefs/x/common.md";
    const { container, onNavigate } = renderMarkdown(
      `See \`${listed}\` and \`${listed.slice(4)}\` and \`doc/unlisted.md\`.\n`,
      { docPaths: new Set([PATH, listed]) },
    );
    const links = Array.from(container.querySelectorAll("a > code")).map((code) => [
      code.textContent,
      code.parentElement?.getAttribute("href"),
    ]);
    expect(links).toEqual([
      [listed, "/files/plans/briefs/x/common.md"],
      [listed.slice(4), "/files/plans/briefs/x/common.md"],
    ]);
    fireEvent.click(container.querySelector("a")!);
    expect(onNavigate).toHaveBeenCalledWith("/files/plans/briefs/x/common.md");
  });

  it("renders an image inside a link as text, so the link is the only anchor", () => {
    const { container, onNavigate } = renderMarkdown("[![badge](b.png)](other.md)\n");
    const anchors = container.querySelectorAll("a");
    expect(anchors).toHaveLength(1);
    expect(anchors[0]?.textContent).toBe("badge");
    fireEvent.click(anchors[0]!);
    expect(onNavigate).toHaveBeenCalledTimes(1);
  });
});
