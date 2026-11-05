import { useId, useState, type JSX } from "react";
import {
  Bot,
  Check,
  FileText,
  Pencil,
  Search,
  Terminal,
  Wrench,
  X,
  type LucideIcon,
} from "lucide-react";

import type { Item } from "@/plan/api/schema-chat";
import { PatchView } from "./patch-view";
import { Untrusted } from "./visible-controls";

type ToolItem = Extract<Item, { type: "tool" }>;

/** The glyph that stands for a tool in its row; a tool without an entry gets the wrench. */
const TOOL_GLYPHS: Record<string, LucideIcon> = {
  Read: FileText,
  Edit: Pencil,
  Write: Pencil,
  MultiEdit: Pencil,
  Bash: Terminal,
  Grep: Search,
  Glob: Search,
  Agent: Bot,
};

function toolGlyph(name: string): LucideIcon {
  return Object.hasOwn(TOOL_GLYPHS, name) ? TOOL_GLYPHS[name]! : Wrench;
}

const STATUS_WORD: Record<ToolItem["status"], string> = {
  pending: "Pending",
  done: "Done",
  error: "Error",
  denied: "Denied",
  not_finished: "Not finished",
};

/** The word keeps the row's colour; only the glyph beside it takes a tone (chat-tools.css). */
function ToolStatus({ status }: { status: ToolItem["status"] }): JSX.Element {
  return (
    <span className="chat-tool-status" data-status={status}>
      {status === "error" && <X size={13} aria-hidden="true" />}
      {status === "done" && <Check size={13} aria-hidden="true" />}
      {STATUS_WORD[status]}
    </span>
  );
}

/**
 * Lines added and removed, as a tool row and the left-behind card end: `+N` in the completed
 * tone, `−M` in the destructive one (chat-tools.css). Nothing when the bridge reported neither.
 */
export function ChangeCounts({
  added,
  removed,
}: {
  added: number | null;
  removed: number | null;
}): JSX.Element | null {
  if (added === null && removed === null) return null;
  return (
    <span className="chat-counts">
      <span className="chat-count-added">+{added ?? 0}</span>{" "}
      <span className="chat-count-removed">−{removed ?? 0}</span>
    </span>
  );
}

function ToolOutput({
  text,
  patch = false,
  truncated = false,
}: {
  text: string;
  patch?: boolean;
  truncated?: boolean;
}): JSX.Element {
  const [showAll, setShowAll] = useState(false);
  const lines = text.split("\n");
  const capped = lines.length > 40;
  const shown = capped && !showAll ? lines.slice(0, 40).join("\n") : text;
  return (
    <div className="min-w-0 space-y-1">
      {patch ? (
        <PatchView patch={shown} />
      ) : (
        <pre className="chat-tool-pre">
          <Untrusted text={shown} />
        </pre>
      )}
      {capped && !showAll && (
        <button type="button" className="chat-link-button" onClick={() => setShowAll(true)}>
          Show all
        </button>
      )}
      {truncated && <p className="plan-fine">Result truncated.</p>}
    </div>
  );
}

function ToolDetails({ item }: { item: ToolItem }): JSX.Element {
  const showPatch = (item.name === "Edit" || item.name === "Write") && item.patch !== null;
  return (
    <div className="chat-tool-details">
      <div>
        <p className="chat-tool-details-label">Input</p>
        <pre className="chat-tool-pre">
          <Untrusted text={item.input} />
        </pre>
      </div>
      {showPatch && (
        <div>
          <p className="chat-tool-details-label">Patch</p>
          <ToolOutput text={item.patch!} patch />
        </div>
      )}
      {item.result !== null && (
        <div>
          <p className="chat-tool-details-label">Result</p>
          <ToolOutput text={item.result} truncated={item.result_truncated} />
        </div>
      )}
    </div>
  );
}

function ToolNote({ item }: { item: ToolItem }): JSX.Element | null {
  const text = item.error_line ?? item.result;
  if (text === null || (item.status !== "error" && item.status !== "denied")) return null;
  const note = item.status === "error" ? text.split(/\r\n|[\n\r\u2028\u2029]/, 1)[0] : text;
  return (
    <p className="chat-tool-note">
      <Untrusted text={note} />
    </p>
  );
}

function ToolHeader({
  item,
  expanded,
  detailsId,
  onToggle,
}: {
  item: ToolItem;
  expanded: boolean;
  detailsId: string;
  onToggle: () => void;
}): JSX.Element {
  // One line: the tool's glyph, its target (the bridge's summary names the verb), and at the end
  // the counts of a finished edit or the status word. The row is a button that opens its details.
  const Glyph = toolGlyph(item.name);
  const counted = item.status === "done" && (item.added !== null || item.removed !== null);
  return (
    <button
      type="button"
      aria-expanded={expanded}
      aria-controls={detailsId}
      className="chat-tool-head"
      data-status={item.status}
      onClick={onToggle}
    >
      <span className="chat-tool-glyph">
        <Glyph size={14} aria-hidden="true" />
        <span className="sr-only">
          <Untrusted text={item.name} />
        </span>
      </span>{" "}
      <span className="chat-tool-target">
        <Untrusted text={item.summary} />
      </span>
      {item.name === "Agent" && (
        <span className="chat-tool-extra">
          {item.nested_tools} tool {item.nested_tools === 1 ? "call" : "calls"}
        </span>
      )}
      {counted ? (
        <ChangeCounts added={item.added} removed={item.removed} />
      ) : (
        <ToolStatus status={item.status} />
      )}
    </button>
  );
}

export function ToolRow({
  item,
  nested = [],
}: {
  item: ToolItem;
  nested?: ToolItem[];
}): JSX.Element {
  const [expanded, setExpanded] = useState(false);
  const detailsId = useId();
  const children =
    item.name === "Agent" ? nested.filter((child) => child.parent === item.tool_use_id) : [];
  return (
    <div className="chat-tool" aria-live="off">
      <ToolHeader
        item={item}
        expanded={expanded}
        detailsId={detailsId}
        onToggle={() => setExpanded((value) => !value)}
      />
      <ToolNote item={item} />
      <div id={detailsId} hidden={!expanded} className="min-w-0">
        {expanded && <ToolDetails key={item.id} item={item} />}
        {expanded && children.length > 0 && (
          <div className="chat-tool-nested min-w-0 pl-4">
            {children.map((child) => (
              <ToolRow key={child.id} item={child} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
