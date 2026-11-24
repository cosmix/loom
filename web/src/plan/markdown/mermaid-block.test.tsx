import { act, cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { CodeBlock } from "@/plan/markdown/code-block";

const mermaid = vi.hoisted(() => ({
  initialize: vi.fn(),
  render: vi.fn<(id: string, source: string) => Promise<{ svg: string }>>(),
}));
vi.mock("mermaid", () => ({ default: mermaid }));

const SOURCE = "graph TD\n  a --> b";

beforeEach(() => {
  mermaid.initialize.mockReset();
  mermaid.render.mockReset().mockResolvedValue({ svg: '<svg data-testid="drawn"></svg>' });
});

afterEach(() => {
  cleanup();
  document.documentElement.className = "";
});

describe("mermaid fences", () => {
  it("shows the source while loading, then the drawn diagram", async () => {
    const { container } = render(<CodeBlock value={SOURCE} lang="mermaid" />);
    expect(container.querySelector("pre > code")?.textContent).toBe(SOURCE);

    await waitFor(() => expect(container.querySelector(".md-diagram svg")).not.toBeNull());
    expect(container.querySelector(".md-code")).toBeNull();
    expect(mermaid.render).toHaveBeenCalledWith(expect.stringMatching(/^mmd-/), SOURCE);
    expect(mermaid.initialize).toHaveBeenCalledWith(
      expect.objectContaining({
        startOnLoad: false,
        securityLevel: "strict",
        theme: "base",
        themeVariables: expect.objectContaining({ darkMode: false }),
      }),
    );
  });

  it("falls back to the source with a note when the diagram does not parse", async () => {
    mermaid.render.mockRejectedValue(new Error("Parse error"));
    const { container } = render(<CodeBlock value={SOURCE} lang="mermaid" />);

    await waitFor(() => expect(container.querySelector(".md-code-note")).not.toBeNull());
    expect(container.querySelector("pre > code")?.textContent).toBe(SOURCE);
    expect(container.querySelector(".md-diagram")).toBeNull();
  });

  it("removes images and links from inside foreignObject but keeps the theme style", async () => {
    mermaid.render.mockResolvedValue({
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><style>.a{}</style><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><img src="/api/x"/><a href="https://evil.example">t</a><span>label</span></div></foreignObject></svg>',
    });
    const { container } = render(<CodeBlock value={SOURCE} lang="mermaid" />);

    await waitFor(() => expect(container.querySelector(".md-diagram")).not.toBeNull());
    const diagram = container.querySelector(".md-diagram")!;
    expect(diagram.querySelector("img, a")).toBeNull();
    expect(diagram.textContent).toContain("t");
    expect(diagram.textContent).toContain("label");
    expect(diagram.querySelector("svg > style")).not.toBeNull();
  });

  it("removes form controls from inside foreignObject", async () => {
    mermaid.render.mockResolvedValue({
      svg: '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><form action="https://evil.example"><input name="q"/><button>go</button><textarea>t</textarea><select><option>o</option></select></form><span>label</span></div></foreignObject></svg>',
    });
    const { container } = render(<CodeBlock value={SOURCE} lang="mermaid" />);

    await waitFor(() => expect(container.querySelector(".md-diagram")).not.toBeNull());
    const diagram = container.querySelector(".md-diagram")!;
    expect(diagram.querySelector("form, input, button, textarea, select")).toBeNull();
    expect(diagram.textContent).toContain("label");
  });

  it("falls back to the source when the rendered markup holds no svg", async () => {
    mermaid.render.mockResolvedValue({ svg: "<p>not a diagram</p>" });
    const { container } = render(<CodeBlock value={SOURCE} lang="mermaid" />);

    await waitFor(() => expect(container.querySelector(".md-code-note")).not.toBeNull());
    expect(container.querySelector(".md-code-note")?.textContent).toBe(
      "This diagram could not be drawn.",
    );
    expect(container.querySelector(".md-diagram")).toBeNull();
  });

  it("redraws with dark variables when the document theme changes", async () => {
    const { container } = render(<CodeBlock value={SOURCE} lang="mermaid" />);
    await waitFor(() => expect(container.querySelector(".md-diagram")).not.toBeNull());
    expect(mermaid.render).toHaveBeenCalledTimes(1);

    act(() => document.documentElement.classList.add("dark"));
    await waitFor(() => expect(mermaid.render).toHaveBeenCalledTimes(2));
    expect(mermaid.initialize).toHaveBeenLastCalledWith(
      expect.objectContaining({
        themeVariables: expect.objectContaining({ darkMode: true }),
      }),
    );
  });

  it("leaves other languages as plain source", () => {
    const { container } = render(<CodeBlock value="fn main() {}" lang="rust" />);
    expect(container.querySelector(".md-code-lang")?.textContent).toBe("rust");
    expect(mermaid.render).not.toHaveBeenCalled();
  });
});
