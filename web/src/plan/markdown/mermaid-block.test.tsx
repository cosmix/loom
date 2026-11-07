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
