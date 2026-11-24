import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { CodeBlock } from "@/plan/markdown/code-block";

const mermaid = vi.hoisted(() => ({
  initialize: vi.fn(),
  render: vi.fn<(id: string, source: string) => Promise<{ svg: string }>>(),
}));
vi.mock("mermaid", () => ({ default: mermaid }));

const LABEL_SVG =
  '<svg xmlns="http://www.w3.org/2000/svg"><foreignObject>' +
  '<div xmlns="http://www.w3.org/1999/xhtml"><img src="/api/x"/>' +
  '<a href="https://evil.example">t</a><span>label</span></div>' +
  "</foreignObject></svg>";

beforeEach(() => {
  mermaid.initialize.mockReset();
  mermaid.render.mockReset().mockResolvedValue({ svg: LABEL_SVG });
});

afterEach(() => {
  cleanup();
  document.documentElement.className = "";
});

describe("mermaid diagram contract", () => {
  it("a foreign-object image in a label is rendered without the image", async () => {
    const { container } = render(<CodeBlock value={"graph TD\n  a --> b"} lang="mermaid" />);

    await waitFor(() => expect(container.querySelector(".md-diagram")).not.toBeNull());
    const diagram = container.querySelector(".md-diagram") as HTMLElement;
    await waitFor(() => expect(diagram.textContent).toContain("label"));

    expect(diagram.querySelector("img")).toBeNull();
    expect(diagram.querySelector("a")).toBeNull();
    expect(diagram.textContent).toContain("label");
    expect(diagram.textContent).toContain("t");
  });
});
