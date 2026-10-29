import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { renderSummaryInlines } from "@/plan/markdown/inline";

import { SentenceText } from "./sentence-text";

afterEach(cleanup);

describe("SentenceText", () => {
  it.each([
    ["`code`", "code", "code"],
    ["*em*", "em", "em"],
    ["**strong**", "strong", "strong"],
  ])("renders %s as <%s>", (text, tag, expected) => {
    const { container } = render(<SentenceText text={text} />);

    expect(container.querySelector(tag)?.textContent).toBe(expected);
    expect(container.textContent).toBe(expected);
  });

  it("preserves nested emphasis, strong and code", () => {
    const { container } = render(<SentenceText text="*An **important `path`** change*" />);

    expect(container.querySelector("em > strong > code")?.textContent).toBe("path");
    expect(container.textContent).toBe("An important path change");
  });

  it("renders inline code in a numbered title while keeping the literal prefix", () => {
    const { container } = render(
      <SentenceText text="3. File mode (`PLAN-feat-planner-03-file-mode.md`)" />,
    );

    expect(container.querySelector("code")?.textContent).toBe("PLAN-feat-planner-03-file-mode.md");
    expect(container.textContent).toBe("3. File mode (PLAN-feat-planner-03-file-mode.md)");
    expect(container.textContent).not.toContain("`");
    expect(container.querySelector("ol, li, a")).toBeNull();
  });

  it.each([
    "Read [the *guide*](https://example.com).",
    "Read [the *guide*][guide].\n\n[guide]: https://example.com",
  ])("renders link labels without anchors: %s", (text) => {
    const { container } = render(<SentenceText text={text} />);

    expect(container.textContent).toBe("Read the guide.");
    expect(container.querySelector("em")?.textContent).toBe("guide");
    expect(container.querySelector("a")).toBeNull();
  });

  it.each([
    "<script>alert(1)</script>",
    '<div onclick="alert(1)">raw HTML</div>',
    "Read <b>raw HTML</b> here.",
  ])("renders HTML as inert text: %s", (text) => {
    const { container } = render(<SentenceText text={text} />);

    expect(container.textContent).toBe(text);
    expect(container.querySelector("*")).toBeNull();
  });

  it("hides HTML comments through the existing raw HTML rules", () => {
    const { container } = render(<SentenceText text="Before <!-- hidden --> after." />);

    expect(container.textContent).toBe("Before  after.");
    expect(container.querySelector("*")).toBeNull();
  });

  it.each([
    "![Architecture diagram](https://example.com/diagram.png)",
    "![Architecture diagram][diagram]\n\n[diagram]: https://example.com/diagram.png",
  ])("renders image alt text without images or links: %s", (text) => {
    const { container } = render(<SentenceText text={text} />);

    expect(container.textContent).toBe("Architecture diagram");
    expect(container.querySelector("img, a")).toBeNull();
  });

  it("leaves plain text unchanged", () => {
    const { container } = render(<SentenceText text="Foldable one stays readable." />);

    expect(container.textContent).toBe("Foldable one stays readable.");
    expect(container.querySelector("*")).toBeNull();
  });

  it("joins paragraphs with one space without adding block elements", () => {
    const { container } = render(
      <SentenceText text={"First *paragraph*.\n\nSecond `paragraph`."} />,
    );

    expect(container.textContent).toBe("First paragraph. Second paragraph.");
    expect(container.querySelector("p")).toBeNull();
  });

  it("keeps the source text, markers included, of non-paragraph blocks", () => {
    const { container } = render(
      <SentenceText text={"# A **heading**\n\n```ts\nconst value = 1;\n```\n\nLast paragraph."} />,
    );

    expect(container.textContent).toBe(
      "# A **heading** ```ts\nconst value = 1;\n``` Last paragraph.",
    );
    expect(container.querySelector("*")).toBeNull();
  });

  it.each(["1. Install loom before planning.", "> Quoted claim.", "- Item one"])(
    "keeps the leading marker of %j",
    (text) => {
      const { container } = render(<SentenceText text={text} />);

      expect(container.textContent).toBe(text);
    },
  );

  it.each(["", " \n "])("preserves input when parsing yields no content: %j", (text) => {
    const { container } = render(<SentenceText text={text} />);

    expect(container.textContent).toBe(text);
  });

  it("updates the parsed content when text changes", () => {
    const { container, rerender } = render(<SentenceText text="*Before*" />);

    rerender(<SentenceText text="**After**" />);

    expect(container.querySelector("strong")?.textContent).toBe("After");
    expect(container.querySelector("em")).toBeNull();
  });
});

describe("renderSummaryInlines", () => {
  it("degrades deletion and hard breaks without extra elements", () => {
    const { container } = render(
      <>
        {renderSummaryInlines([
          { type: "delete", children: [{ type: "text", value: "Deleted" }] },
          { type: "break" },
          { type: "text", value: "text" },
        ])}
      </>,
    );

    expect(container.textContent).toBe("Deleted text");
    expect(container.querySelector("*")).toBeNull();
  });
});
