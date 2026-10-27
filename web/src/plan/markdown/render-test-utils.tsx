import { render } from "@testing-library/react";
import { expect, vi } from "vitest";

import type { SectionView } from "@/plan/api/schema-files";
import { Markdown, type MarkdownProps } from "@/plan/markdown";

export const PATH = "doc/notes.md";
const ALLOWED_PROTOCOLS = ["http:", "https:", "mailto:"];

export function section(anchor: string, startLine: number, level = 2): SectionView {
  return { level, title: anchor, anchor, start_line: startLine, end_line: startLine + 1 };
}

export function renderMarkdown(
  source: string,
  props: Partial<MarkdownProps> = {},
): { container: HTMLElement; onNavigate: ReturnType<typeof vi.fn> } {
  const onNavigate = vi.fn();
  const { container } = render(
    <Markdown
      source={source}
      path={PATH}
      sections={[]}
      onNavigate={onNavigate}
      docPaths={new Set([PATH])}
      anchorsOf={() => null}
      {...props}
    />,
  );
  return { container, onNavigate };
}

/** The anchor whose text is exactly `label`, or undefined. */
export function linkNamed(container: HTMLElement, label: string): HTMLAnchorElement | undefined {
  return Array.from(container.querySelectorAll("a")).find(
    (anchor) => anchor.textContent?.trim() === label,
  );
}

/** Nothing in the container can run script or fetch a resource. */
export function expectInert(container: HTMLElement): void {
  expect(container.querySelector("script, img, iframe, object, embed, picture, video")).toBeNull();
  expect(container.querySelector("[src], [srcset], style, link, meta, form")).toBeNull();
  for (const element of Array.from(container.querySelectorAll("*"))) {
    for (const attribute of Array.from(element.attributes)) {
      expect(attribute.name.toLowerCase().startsWith("on"), attribute.name).toBe(false);
    }
  }
  for (const anchor of Array.from(container.querySelectorAll("a"))) {
    const protocol = new URL(anchor.getAttribute("href") ?? "", window.location.href).protocol;
    expect(ALLOWED_PROTOCOLS, anchor.getAttribute("href") ?? "").toContain(protocol);
  }
}
