import { cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import hostile from "@/plan/markdown/fixtures/hostile.md?raw";
import { expectInert, linkNamed, renderMarkdown } from "@/plan/markdown/render-test-utils";

afterEach(cleanup);

const TEXT_LINKS = ["js1", "js2", "js3", "js4", "js5", "js10"];

// Character references in a destination decode before the scheme is read. These
// live here, not in a fixture: the formatter decodes references in .md files.
const ENCODED_SCHEMES = [
  "[e1](java&#x09;script:alert(1))",
  "[e2](&#32;javascript:alert(1))",
  "[e3](&#x6A;avascript:alert(1))",
  "[e4](javascript&#58;alert(1))",
  "[e5](&#106;&#97;&#118;&#97;&#115;&#99;&#114;&#105;&#112;&#116;&#58;alert(1))",
  "[e6](javascript&colon;alert(1))",
];

describe("Markdown against hostile input", () => {
  it("renders nothing that can run or fetch", () => {
    const { container } = renderMarkdown(hostile);
    expectInert(container);
  });

  it("shows raw html as its literal text", () => {
    const { container } = renderMarkdown(hostile);
    const text = container.textContent ?? "";
    expect(text).toContain("<script>alert(1)</script>");
    expect(text).toContain("<script>alert(2)</script>");
    expect(text).toContain("<img src=x onerror=alert(1)>");
    expect(text).toContain('<a href="javascript:alert(1)">');
    expect(text).toContain('<iframe src="https://evil.example"></iframe>');
    expect(linkNamed(container, "raw")).toBeUndefined();
  });

  it("drops html comments, hiding the metadata markers", () => {
    const { container } = renderMarkdown(hostile);
    expect(container.textContent).not.toContain("loom METADATA");
    expect(container.textContent).not.toContain("<!--");
  });

  it("renders every disallowed scheme as plain text", () => {
    const { container } = renderMarkdown(hostile);
    const text = container.textContent ?? "";
    for (const label of TEXT_LINKS) {
      expect(text, label).toContain(label);
      expect(linkNamed(container, label), label).toBeUndefined();
    }
  });

  it("renders a character-encoded script scheme as plain text", () => {
    const { container } = renderMarkdown(ENCODED_SCHEMES.join("\n\n") + "\n");
    expectInert(container);
    ENCODED_SCHEMES.forEach((_, index) => {
      const label = `e${index + 1}`;
      expect(container.textContent, label).toContain(label);
      expect(linkNamed(container, label), label).toBeUndefined();
    });
  });

  it("opens a protocol-relative link as external with noopener", () => {
    const { container } = renderMarkdown(hostile);
    const link = linkNamed(container, "ext");
    expect(link?.getAttribute("href")).toBe("https://evil.example/");
    expect(link?.getAttribute("target")).toBe("_blank");
    expect(link?.getAttribute("rel")).toBe("noopener noreferrer");
  });

  it("treats angle and bare autolinks as external links", () => {
    const { container } = renderMarkdown(hostile);
    for (const [label, href] of [
      ["https://x.example", "https://x.example/"],
      ["https://example.com", "https://example.com/"],
    ]) {
      const link = linkNamed(container, label);
      expect(link?.getAttribute("href"), label).toBe(href);
      expect(link?.getAttribute("rel"), label).toBe("noopener noreferrer");
      expect(link?.getAttribute("target"), label).toBe("_blank");
    }
  });

  // The fixture wraps its autolinks in angle brackets (the formatter rewrites bare ones), so a
  // GFM literal URL is covered here with an inline source.
  it("links a bare GFM literal URL and leaves a bare script scheme as text", () => {
    const { container } = renderMarkdown(
      "See https://example.com/x for details.\n\nThen javascript:alert(1) stays put.\n",
    );
    expectInert(container);
    const link = linkNamed(container, "https://example.com/x");
    expect(link?.getAttribute("href")).toBe("https://example.com/x");
    expect(link?.getAttribute("rel")).toBe("noopener noreferrer");
    expect(link?.getAttribute("target")).toBe("_blank");
    expect(container.textContent).toContain("javascript:alert(1)");
    expect(container.querySelectorAll("a")).toHaveLength(1);
  });

  it("renders an image as a link to its target and loads nothing", () => {
    const { container } = renderMarkdown(hostile);
    expect(container.querySelector("img")).toBeNull();
    const link = linkNamed(container, "t");
    expect(link?.getAttribute("href")).toBe("https://tracker.example/p.gif");
    expect(link?.getAttribute("rel")).toBe("noopener noreferrer");
  });

  it("an image pointing at a script scheme is plain alt text", () => {
    const { container } = renderMarkdown("![pic](javascript:alert(1))\n");
    expect(container.querySelector("a, img")).toBeNull();
    expect(container.textContent).toBe("pic");
  });

  it("keeps html inside a table cell and a code span as text", () => {
    const { container } = renderMarkdown(hostile);
    const cell = container.querySelector(".md-table-scroll td");
    expect(cell?.textContent).toBe('<b onclick="alert(1)">cell</b>');
    expect(cell?.querySelector("b")).toBeNull();
    const code = Array.from(container.querySelectorAll("p > code")).find((element) =>
      element.textContent?.includes("alert(3)"),
    );
    expect(code?.textContent).toBe("<script>alert(3)</script>");
  });

  it("strikes through a relative link to a missing document", () => {
    const { container } = renderMarkdown(hostile);
    const link = linkNamed(container, "gone");
    expect(link?.getAttribute("href")).toBe("/files/missing.md");
    expect(link?.className).toContain("md-missing");
    expect(link?.getAttribute("title")).toBe("not found");
  });
});
