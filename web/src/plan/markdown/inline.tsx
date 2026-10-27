import type { PhrasingContent } from "mdast";
import { Fragment, type JSX, type MouseEvent, type ReactNode } from "react";

import { repoRel, resolveLink, toAddress } from "@/plan/files/address";
import { rawHtmlText, type RenderContext } from "@/plan/markdown/context";

/** A plain left click; a modified or middle click keeps the browser's own behaviour. */
function isPlainClick(event: MouseEvent<HTMLAnchorElement>): boolean {
  return (
    !event.defaultPrevented &&
    event.button === 0 &&
    !event.metaKey &&
    !event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey
  );
}

function missingProps(missing: boolean): { className?: string; title?: string } {
  return missing ? { className: "md-missing", title: "not found" } : {};
}

function DocLink(props: {
  path: string;
  anchor: string | null;
  context: RenderContext;
  children: ReactNode;
}): JSX.Element {
  const { path, anchor, context } = props;
  const address = toAddress(path, anchor);
  const missing =
    !context.docPaths.has(path) ||
    (anchor !== null && context.anchorsOf(path)?.has(anchor) === false);
  const follow = (event: MouseEvent<HTMLAnchorElement>) => {
    if (!isPlainClick(event)) return;
    event.preventDefault();
    context.onNavigate(address);
  };
  return (
    <a href={address} onClick={follow} {...missingProps(missing)}>
      {props.children}
    </a>
  );
}

/**
 * A `#anchor` link scrolls to the heading in the same rendered document. When
 * that document is not on screen with ids (an excerpt, chat text), it opens
 * the document at the anchor instead.
 */
function AnchorLink(props: {
  anchor: string;
  context: RenderContext;
  children: ReactNode;
}): JSX.Element {
  const { anchor, context } = props;
  const address = toAddress(context.path, anchor);
  const missing = context.anchorsOf(context.path)?.has(anchor) === false;
  const follow = (event: MouseEvent<HTMLAnchorElement>) => {
    if (!isPlainClick(event)) return;
    event.preventDefault();
    const scope = event.currentTarget.closest(".md");
    const target = Array.from(scope?.querySelectorAll("[id]") ?? []).find(
      (element) => element.id === anchor,
    );
    if (target) target.scrollIntoView({ block: "start" });
    else context.onNavigate(address);
  };
  return (
    <a href={address} onClick={follow} {...missingProps(missing)}>
      {props.children}
    </a>
  );
}

/** Every link, autolink and image target goes through `resolveLink`; nothing else builds an href. */
export function SafeLink(props: {
  url: string;
  context: RenderContext;
  children: ReactNode;
}): JSX.Element {
  const { context, children } = props;
  const link = resolveLink(context.path, props.url);
  switch (link.kind) {
    case "doc":
      return (
        <DocLink path={link.path} anchor={link.anchor} context={context}>
          {children}
        </DocLink>
      );
    case "anchor":
      return (
        <AnchorLink anchor={link.anchor} context={context}>
          {children}
        </AnchorLink>
      );
    case "external":
      return (
        <a href={link.href} target="_blank" rel="noopener noreferrer">
          {children}
        </a>
      );
    case "outside":
      return (
        <span className="md-outside" title={link.target}>
          {children}
        </span>
      );
    case "text":
      return <>{children}</>;
  }
}

function markText(value: string, pattern: RegExp | null): ReactNode {
  if (pattern === null) return value;
  const parts: ReactNode[] = [];
  let last = 0;
  for (const match of value.matchAll(pattern)) {
    const start = match.index;
    if (start > last) parts.push(value.slice(last, start));
    parts.push(<mark key={start}>{match[0]}</mark>);
    last = start + match[0].length;
  }
  if (parts.length === 0) return value;
  if (last < value.length) parts.push(value.slice(last));
  return parts;
}

/** Inline code that is exactly a document's path links to that document. */
function CodeSpan({ value, context }: { value: string; context: RenderContext }): JSX.Element {
  const target = repoRel(value);
  if (!context.docPaths.has(target)) return <code>{value}</code>;
  return (
    <DocLink path={target} anchor={null} context={context}>
      <code>{value}</code>
    </DocLink>
  );
}

function referenceUrl(identifier: string, context: RenderContext): string | null {
  return context.definitions.get(identifier)?.url ?? null;
}

/** `insideLink`: the nodes are a link's children, where an image is plain text (an `<a>` cannot nest). */
function renderInline(
  node: PhrasingContent,
  context: RenderContext,
  insideLink: boolean,
): ReactNode {
  switch (node.type) {
    case "text":
      return markText(node.value, context.marks);
    case "emphasis":
      return <em>{renderInlines(node.children, context, insideLink)}</em>;
    case "strong":
      return <strong>{renderInlines(node.children, context, insideLink)}</strong>;
    case "delete":
      return <del>{renderInlines(node.children, context, insideLink)}</del>;
    case "inlineCode":
      return <CodeSpan value={node.value} context={context} />;
    case "break":
      return <br />;
    case "html":
      return rawHtmlText(node.value);
    case "footnoteReference":
      return <sup className="md-footnote-ref">[{node.label ?? node.identifier}]</sup>;
    case "link":
      return (
        <SafeLink url={node.url} context={context}>
          {renderInlines(node.children, context, true)}
        </SafeLink>
      );
    case "image":
      // Never an <img>: the app loads nothing a document points at.
      if (insideLink) return node.alt || node.url;
      return (
        <SafeLink url={node.url} context={context}>
          {node.alt || node.url}
        </SafeLink>
      );
    case "linkReference":
    case "imageReference":
      return renderReference(node, context, insideLink);
  }
}

function renderReference(
  node: Extract<PhrasingContent, { type: "linkReference" | "imageReference" }>,
  context: RenderContext,
  insideLink: boolean,
): ReactNode {
  const label =
    node.type === "linkReference"
      ? renderInlines(node.children, context, true)
      : node.alt || node.label || node.identifier;
  const url = referenceUrl(node.identifier, context);
  if (url === null || (insideLink && node.type === "imageReference")) return label;
  return (
    <SafeLink url={url} context={context}>
      {label}
    </SafeLink>
  );
}

export function renderInlines(
  nodes: readonly PhrasingContent[],
  context: RenderContext,
  insideLink = false,
): ReactNode[] {
  return nodes.map((node, index) => (
    <Fragment key={index}>{renderInline(node, context, insideLink)}</Fragment>
  ));
}
