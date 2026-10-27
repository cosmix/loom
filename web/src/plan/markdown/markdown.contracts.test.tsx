import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import type { SectionView } from "@/plan/api/schema-files";
import { Markdown } from "@/plan/markdown";

const PATH = "doc/notes.md";
const BLOCKED_SCHEMES = ["javascript:", "data:", "vbscript:"];

function renderMarkdown(source: string, sections: SectionView[] = []): HTMLElement {
  const { container } = render(
    <Markdown
      source={source}
      path={PATH}
      sections={sections}
      onNavigate={() => {}}
      docPaths={new Set([PATH])}
      anchorsOf={() => new Set<string>()}
    />,
  );
  return container;
}

function onAttributes(container: HTMLElement): string[] {
  const found: string[] = [];
  for (const element of Array.from(container.querySelectorAll("*"))) {
    for (const attribute of Array.from(element.attributes)) {
      if (attribute.name.toLowerCase().startsWith("on")) found.push(attribute.name);
    }
  }
  return found;
}

// A browser strips ASCII whitespace and control characters before reading the
// scheme, so the check normalises the same way and also asks the URL parser.
function isBlockedHref(href: string): boolean {
  const squeezed = Array.from(href)
    .filter((char) => char.charCodeAt(0) > 0x20)
    .join("")
    .toLowerCase();
  if (BLOCKED_SCHEMES.some((scheme) => squeezed.startsWith(scheme))) return true;
  try {
    return BLOCKED_SCHEMES.includes(new URL(href, window.location.href).protocol);
  } catch {
    return false;
  }
}

afterEach(() => {
  cleanup();
});

describe("markdown contracts", () => {
  it("markdown renders raw html as escaped text", () => {
    const source = [
      "<script>alert(1)</script>",
      "",
      'Inline <img src=x onerror=alert(1)> here and <a href="javascript:alert(1)">x</a> too.',
      "",
    ].join("\n");
    const container = renderMarkdown(source);

    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("iframe, object, embed")).toBeNull();
    expect(onAttributes(container)).toEqual([]);
    for (const anchor of Array.from(container.querySelectorAll("a"))) {
      expect(isBlockedHref(anchor.getAttribute("href") ?? "")).toBe(false);
    }
    const text = container.textContent ?? "";
    expect(text).toContain("<script>alert(1)</script>");
    expect(text).toContain("<img src=x onerror=alert(1)>");
    expect(text).toContain('<a href="javascript:alert(1)">');
  });

  it("markdown renders a javascript link as plain text", () => {
    const source = [
      "[x](JaVaScRiPt:alert(1))",
      "",
      "[y](data:text/html,hi)",
      "",
      "[z](java&#x09;script:alert(1))",
      "",
      "[w](&#32;javascript:alert(1))",
      "",
    ].join("\n");
    const container = renderMarkdown(source);

    for (const anchor of Array.from(container.querySelectorAll("a"))) {
      expect(isBlockedHref(anchor.getAttribute("href") ?? "")).toBe(false);
      expect(isBlockedHref(anchor.href)).toBe(false);
    }
    expect(container.querySelector("script, iframe, object, embed")).toBeNull();
    expect(onAttributes(container)).toEqual([]);
    const text = container.textContent ?? "";
    for (const label of ["x", "y", "z", "w"]) {
      expect(text).toContain(label);
    }
  });

  it("markdown renders an image as a link and loads nothing", () => {
    const container = renderMarkdown("![pixel](https://tracker.example/p.gif)\n");

    expect(container.querySelector("img, picture, source, video, audio, iframe")).toBeNull();
    expect(container.querySelector("[src], [srcset]")).toBeNull();
    const links = Array.from(container.querySelectorAll("a")).filter(
      (anchor) => anchor.textContent?.trim() === "pixel",
    );
    expect(links).toHaveLength(1);
    expect(links[0].getAttribute("href")).toBe("https://tracker.example/p.gif");
  });
});
