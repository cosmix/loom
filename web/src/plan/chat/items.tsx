import { useAtomValue } from "jotai/react";
import { CopyIcon } from "lucide-react";
import type { Nodes } from "mdast";
import { useId, useMemo, useState, type JSX } from "react";
import { useNavigate } from "react-router";

import type { Item, Turn } from "@/plan/api/schema-chat";
import { SentChips } from "@/plan/chat/chips";
import { snapshotAtom } from "@/plan/chat/model";
import { PermissionCard } from "@/plan/chat/permission-card";
import { ResendControl } from "@/plan/chat/resend-control";
import { ToolRow } from "@/plan/chat/tool-row";
import { useSessionBrief } from "@/plan/chat/use-session-brief";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { resolveLink } from "@/plan/files/address";
import { openDocAtom, treeAtom } from "@/plan/files/atoms";
import { docPathsOf } from "@/plan/files/tree";
import { CopyButton } from "@/plan/markdown/copy-button";
import { Markdown, parseMarkdown } from "@/plan/markdown/markdown";

type ItemOf<T extends Item["type"]> = Extract<Item, { type: T }>;
type Destination = Extract<Nodes, { type: "link" | "image" | "definition" }>;
const CHAT_PATH = "doc/README.md";

function escapeLabel(text: string): string {
  return text.replace(/[\\`*_[\]<>!&]/g, (character) => `&#${character.codePointAt(0)};`);
}

/** Only rewritten destinations are serialized; message/code source otherwise stays intact. */
function destinationSource(node: Destination, source: string, url: string): string {
  const target = url.replace(/[&<>\\\r\n]/g, (character) =>
    character === "&" ? "&amp;" : encodeURIComponent(character),
  );
  const title = node.title == null ? "" : ` "${node.title.replace(/[\\"]/g, "\\$&")}"`;
  if (node.type === "definition") {
    const start = node.position?.start.offset ?? 0;
    let end = start + 1;
    while (end < source.length && source[end] !== "]") {
      end += source[end] === "\\" ? 2 : 1;
    }
    return `${source.slice(start, end + 2)} <${target}>${title}`;
  }
  if (node.type === "image") {
    return `![${escapeLabel(node.alt ?? "")}](<${target}>${title})`;
  }
  // A GFM literal autolink (a@b.com) has no opening bracket, so the label starts at the first
  // child; for `[label](url)` and `<url>` that child sits one character past the node start.
  const start = node.position?.start.offset ?? 0;
  const labelStart = node.children[0]?.position?.start.offset ?? start + 1;
  const labelEnd = node.children.at(-1)?.position?.end.offset ?? labelStart;
  return `[${source.slice(labelStart, labelEnd)}](<${target}>${title})`;
}

function chatSource(markdown: string): string {
  const source = visibleControls(markdown);
  const edits: Array<{ start: number; end: number; text: string }> = [];
  const visit = (node: Nodes) => {
    if (node.type === "link" || node.type === "image" || node.type === "definition") {
      // File mode resolves relative URLs from path; repo-prefixed URLs need a root slash.
      let url = node.url.startsWith("doc/") ? `/${node.url}` : node.url;
      const link = resolveLink(CHAT_PATH, url);
      if (link.kind === "external" && !/^https?:/i.test(link.href)) url = "about:blank";
      const start = node.position?.start.offset;
      const end = node.position?.end.offset;
      if (url !== node.url && start !== undefined && end !== undefined) {
        edits.push({ start, end, text: destinationSource(node, source, url) });
        return;
      }
    }
    if ("children" in node) node.children.forEach(visit);
  };
  visit(parseMarkdown(source));
  let result = source;
  for (const edit of edits.sort((a, b) => b.start - a.start)) {
    result = result.slice(0, edit.start) + edit.text + result.slice(edit.end);
  }
  return result;
}

function MessageControls({ source, turn }: { source: string; turn: Turn }): JSX.Element {
  // The bridge timestamps turns, not individual items.
  const date = new Date(turn.started_at);
  const time = Number.isNaN(date.getTime())
    ? turn.started_at
    : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return (
    <div className="flex items-center gap-2 text-xs text-muted-foreground opacity-0 group-hover/message:opacity-100 group-focus-within/message:opacity-100 [@media(hover:none)]:opacity-100">
      <CopyButton text={source} label="Copy Markdown source" icon={CopyIcon} />
      <time dateTime={visibleControls(turn.started_at)} title={visibleControls(turn.started_at)}>
        <Untrusted text={time} />
      </time>
    </div>
  );
}

function OpeningLabel({ turn }: { turn: Turn }): JSX.Element {
  const snapshot = useAtomValue(snapshotAtom);
  const matches = (entry: Turn) => entry.id === turn.id && entry.started_at === turn.started_at;
  const session = snapshot.turns.some(matches)
    ? snapshot.session
    : snapshot.previous?.turns.some(matches)
      ? snapshot.previous.session
      : snapshot.session;
  const brief = useSessionBrief(session?.id ?? null);
  if (session && brief.status === "ready") return <Untrusted text={brief.brief.label} />;
  // Inline text, never a block: this sits in a message header, and a placeholder taller than the
  // line would push the whole transcript around each time a label loads.
  return <Untrusted text="Opening message" />;
}

function DeliveryState({ item, turn }: { item: ItemOf<"user">; turn: Turn }): JSX.Element | null {
  if (item.delivery === "delivered") return null;
  if (item.delivery === "sent") {
    return <p className="text-xs text-muted-foreground">Sending</p>;
  }
  return (
    <div className="flex flex-wrap items-center gap-2 text-xs text-(--tone-warning)">
      <span>Not delivered</span>
      <ResendControl item={item} turn={turn} />
    </div>
  );
}

function UserItem({ item, turn }: { item: ItemOf<"user">; turn: Turn }): JSX.Element {
  const [expanded, setExpanded] = useState(false);
  const bodyId = useId();
  const collapsed = item.origin !== null && !expanded;
  const shown = collapsed ? (item.text.split(/\r\n?|\n/, 1)[0] ?? "") : item.text;
  return (
    <div className="group/message ml-auto w-fit max-w-[85%] space-y-1 font-sans text-sm leading-normal">
      <div className="space-y-2 rounded-lg bg-muted px-3 py-2">
        {item.origin !== null && (
          <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
            <div>
              {item.origin === "closing" ? (
                <Untrusted text="Closing message from loom plan" />
              ) : (
                <OpeningLabel turn={turn} />
              )}
            </div>
            <button
              type="button"
              aria-expanded={expanded}
              aria-controls={bodyId}
              className="shrink-0 rounded px-1 underline underline-offset-2 focus-visible:outline focus-visible:outline-ring"
              onClick={() => setExpanded((value) => !value)}
            >
              {expanded ? "Hide" : "Show"}
            </button>
          </div>
        )}
        <p id={bodyId} className="whitespace-pre-wrap break-words">
          <Untrusted text={shown} />
        </p>
      </div>
      <DeliveryState item={item} turn={turn} />
      <SentChips item={item} turn={turn} />
      <MessageControls source={item.text} turn={turn} />
    </div>
  );
}

function TextItem({ item, turn }: { item: ItemOf<"text">; turn: Turn }): JSX.Element {
  const navigate = useNavigate();
  const tree = useAtomValue(treeAtom);
  const opened = useAtomValue(openDocAtom);
  const paths = useMemo(() => docPathsOf(tree, CHAT_PATH), [tree]);
  const source = useMemo(() => chatSource(item.markdown), [item.markdown]);
  return (
    <div className="group/message w-full min-w-0 font-sans text-sm leading-normal">
      <div
        style={{ unicodeBidi: "plaintext", direction: "ltr" }}
        className="[&>.md]:max-w-none! [&>.md]:text-sm! [&>.md]:leading-normal!"
      >
        <Markdown
          source={source}
          path="doc/README.md"
          sections={[]}
          headingIds={false}
          onNavigate={(address) => {
            void navigate(address);
          }}
          docPaths={paths}
          anchorsOf={(path) =>
            path === opened?.doc.path
              ? new Set(opened.doc.sections.map((section) => section.anchor))
              : null
          }
        />
      </div>
      <MessageControls source={item.markdown} turn={turn} />
    </div>
  );
}

function ThinkingItem({ item }: { item: ItemOf<"thinking"> }): JSX.Element {
  // Transcript merges consecutive markers before passing the item here. A marker without a
  // duration says only that the model thought: "0s" would claim a measurement nobody made.
  return (
    <p className="text-xs text-muted-foreground">
      {item.duration_ms === null
        ? "Thought"
        : `Thought for ${Number((item.duration_ms / 1000).toFixed(1))}s`}
    </p>
  );
}

function NoticeItem({ item }: { item: ItemOf<"notice"> }): JSX.Element {
  const labels = {
    api_error: "API error",
    interrupted: "Interrupted",
    compacted: "Compacted",
    stopped: "Stopped",
    info: "Information",
  };
  const glyph =
    item.kind === "api_error"
      ? "⚠"
      : item.kind === "stopped" || item.kind === "interrupted"
        ? "■"
        : "ⓘ";
  return (
    <p
      className={`flex justify-center gap-1.5 whitespace-pre-wrap break-words text-center text-xs ${
        item.kind === "api_error" ? "text-(--tone-blocked)" : "text-muted-foreground"
      }`}
    >
      <span role="img" aria-label={labels[item.kind]}>
        {glyph}
      </span>
      <Untrusted text={item.text} />
    </p>
  );
}

export function ItemView({
  item,
  turn,
  nested,
}: {
  item: Item;
  turn: Turn;
  nested?: ItemOf<"tool">[];
}): JSX.Element | null {
  switch (item.type) {
    case "user":
      return <UserItem key={item.id} item={item} turn={turn} />;
    case "text":
      return <TextItem item={item} turn={turn} />;
    case "thinking":
      return <ThinkingItem item={item} />;
    case "tool":
      return <ToolRow item={item} nested={nested} />;
    case "permission":
      return <PermissionCard item={item} inline />;
    case "notice":
      return <NoticeItem item={item} />;
  }
}
