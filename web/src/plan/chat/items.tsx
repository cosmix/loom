import { useAtomValue } from "jotai/react";
import { CopyIcon } from "lucide-react";
import type { Nodes } from "mdast";
import { useId, useMemo, useState, type JSX } from "react";
import { useNavigate } from "react-router";

import type { Attachment, Item, Turn } from "@/plan/api/schema-chat";
import { SentChips } from "@/plan/chat/chips";
import { snapshotAtom } from "@/plan/chat/model";
import { PermissionCard } from "@/plan/chat/permission-card";
import { ResendControl } from "@/plan/chat/resend-control";
import { withoutKickoffFence } from "@/plan/chat/kickoff-fence";
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

/**
 * A passage names an anchor, a stage or a line range. A whole document (the viewed page the
 * composer attached) shows its chip alone: its quote is a 2,000-character preview of the file.
 */
function isPassage(attachment: Attachment): boolean {
  return (
    attachment.quote !== "" &&
    (attachment.anchor !== null || attachment.lines !== null || attachment.stage !== null)
  );
}

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
  // The bridge timestamps a turn; an item carries no time of its own.
  const date = new Date(turn.started_at);
  const time = Number.isNaN(date.getTime())
    ? turn.started_at
    : date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  return (
    <div className="chat-message-controls opacity-0 group-hover/message:opacity-100 group-focus-within/message:opacity-100 [@media(hover:none)]:opacity-100">
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
    return <p className="plan-fine">Sending</p>;
  }
  return (
    <div className="flex flex-wrap items-center gap-2 plan-fine">
      <span>
        <span aria-hidden="true" className="text-(--tone-warning)">
          △
        </span>{" "}
        Not delivered
      </span>
      <ResendControl item={item} turn={turn} />
    </div>
  );
}

/**
 * An offer's opening: one quiet centred line, its label and Show, never a bubble. The prompt is
 * not in the DOM until Show, so no text query or screen reader reads it; `bodyId` still names the
 * paragraph Show controls. A closing turn never reaches here: the transcript draws it as a card
 * (closing.ts).
 */
function OpeningItem({ item, turn }: { item: ItemOf<"user">; turn: Turn }): JSX.Element {
  const [expanded, setExpanded] = useState(false);
  const bodyId = useId();
  return (
    <div className="chat-user chat-opening">
      <p className="chat-opening-line">
        <span>
          <OpeningLabel turn={turn} />
        </span>
        <button
          type="button"
          aria-expanded={expanded}
          aria-controls={bodyId}
          className="chat-link-button"
          onClick={() => setExpanded((value) => !value)}
        >
          {expanded ? "Hide" : "Show"}
        </button>
      </p>
      <p id={bodyId} className="chat-opening-prompt">
        {expanded && <Untrusted text={item.text} />}
      </p>
      <DeliveryState item={item} turn={turn} />
      <SentChips item={item} turn={turn} />
    </div>
  );
}

/** The user's words: a bubble on the right, a quoted passage above the words inside it. */
function UserItem({ item, turn }: { item: ItemOf<"user">; turn: Turn }): JSX.Element {
  if (item.origin === "offer") return <OpeningItem item={item} turn={turn} />;
  return (
    <div className="chat-user group/message">
      <div className="chat-user-body">
        {item.attachments.filter(isPassage).map((attachment, index) => (
          <blockquote key={index} className="chat-user-quote">
            <Untrusted text={attachment.quote} />
          </blockquote>
        ))}
        <p className="whitespace-pre-wrap break-words">
          <Untrusted text={item.text} />
        </p>
      </div>
      <DeliveryState item={item} turn={turn} />
      <SentChips item={item} turn={turn} />
      <MessageControls source={item.text} turn={turn} />
    </div>
  );
}

function TextItem({ item, turn }: { item: ItemOf<"text">; turn: Turn }): JSX.Element | null {
  const navigate = useNavigate();
  const tree = useAtomValue(treeAtom);
  const opened = useAtomValue(openDocAtom);
  const paths = useMemo(() => docPathsOf(tree, CHAT_PATH), [tree]);
  // The kickoff block is for the server; the reader sees the prose around it.
  const shown = useMemo(() => withoutKickoffFence(item.markdown), [item.markdown]);
  const source = useMemo(() => chatSource(shown), [shown]);
  if (shown === "") return null;
  return (
    <div className="chat-assistant-text group/message w-full min-w-0">
      <div style={{ unicodeBidi: "plaintext", direction: "ltr" }}>
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
      <MessageControls source={shown} turn={turn} />
    </div>
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
  // A stop or interrupt is drawn as a break in the transcript (chat-transcript.css). Only the
  // glyph takes a tone: an API error is a red mark beside muted words.
  return (
    <p className="chat-notice-item text-center" data-kind={item.kind}>
      <span className="whitespace-pre-wrap break-words">
        <span role="img" aria-label={labels[item.kind]} className="chat-notice-glyph">
          {glyph}
        </span>
        <Untrusted text={item.text} />
      </span>
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
      // The model's thinking is not drawn: the chat shows its words and its work, not its timing.
      return null;
    case "tool":
      return <ToolRow item={item} nested={nested} />;
    case "permission":
      return <PermissionCard item={item} />;
    case "notice":
      return <NoticeItem item={item} />;
  }
}
