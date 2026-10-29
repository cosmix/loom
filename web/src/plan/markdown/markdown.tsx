import type { Root } from "mdast";
import { fromMarkdown } from "mdast-util-from-markdown";
import { gfmFromMarkdown } from "mdast-util-gfm";
import { gfm } from "micromark-extension-gfm";
import { type JSX, useMemo } from "react";

import type { SectionView } from "@/plan/api/schema-files";
import {
  collectDefinitions,
  headingAnchorLookup,
  markPattern,
  type RenderContext,
} from "@/plan/markdown/context";
import { renderBlocks } from "@/plan/markdown/render";

import "@/plan/markdown/markdown.css";

export type MarkdownProps = {
  source: string;
  /** Repo-relative path (`doc/...`) the source belongs to; relative links resolve from it. */
  path: string;
  /** The document's sections; headings take the anchor of the section starting on their line. */
  sections: SectionView[];
  /** Called with a file-mode address when a document link is followed. */
  onNavigate: (address: string) => void;
  /** Case-insensitive whole terms to highlight in text. */
  marks?: string[];
  /** Repo-relative paths of every document file mode can open. */
  docPaths: ReadonlySet<string>;
  /** Anchors of a document, or null when they are not known. */
  anchorsOf: (path: string) => ReadonlySet<string> | null;
  /** File lines before the first line of `source`, for an excerpt. Default 0. */
  lineOffset?: number;
  /** When false, headings carry no id and no copy control. Default true. */
  headingIds?: boolean;
  /** Hide the first top-level H1 when its plain text matches this document title. */
  suppressTitle?: string | null;
};

export function parseMarkdown(source: string): Root {
  return fromMarkdown(source, { extensions: [gfm()], mdastExtensions: [gfmFromMarkdown()] });
}

/**
 * Renders untrusted Markdown to React elements. Raw HTML is shown as text,
 * links pass the file-mode link rules, images become links, and no element is
 * ever built from an HTML string.
 */
export function Markdown({
  source,
  path,
  sections,
  onNavigate,
  marks,
  docPaths,
  anchorsOf,
  lineOffset = 0,
  headingIds = true,
  suppressTitle,
}: MarkdownProps): JSX.Element {
  const root = useMemo(() => parseMarkdown(source), [source]);
  const definitions = useMemo(() => collectDefinitions(root), [root]);
  const headingAnchor = useMemo(
    () => headingAnchorLookup(sections, lineOffset),
    [sections, lineOffset],
  );
  const marked = useMemo(() => markPattern(marks), [marks]);
  const context: RenderContext = {
    path,
    onNavigate,
    docPaths,
    anchorsOf,
    headingAnchor,
    headingIds,
    suppressTitle,
    marks: marked,
    definitions,
  };
  return <div className="md">{renderBlocks(root.children, context)}</div>;
}
