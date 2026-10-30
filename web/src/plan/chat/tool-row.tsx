import { useId, useState, type JSX } from "react";
import { Check, ChevronRight, Wrench, X } from "lucide-react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import type { Item } from "@/plan/api/schema-chat";
import { PatchView } from "./patch-view";
import { Untrusted } from "./visible-controls";

type ToolItem = Extract<Item, { type: "tool" }>;

/** A finished row reads as what happened; a tool without an entry keeps its own name. */
const DONE_VERB: Record<string, string> = { Edit: "Edited", MultiEdit: "Edited", Write: "Wrote" };

function ToolStatus({ status }: { status: ToolItem["status"] }): JSX.Element {
  if (status === "pending") {
    return (
      <span className="inline-flex items-center gap-1 text-muted-foreground">
        <span aria-hidden="true">
          <BusyRoundel busy size={12} busyLabel="" />
        </span>
        Pending
      </span>
    );
  }
  if (status === "error") {
    return (
      <span className="inline-flex items-center gap-1 text-(--tone-blocked)">
        <X size={12} aria-hidden="true" />
        Error
      </span>
    );
  }
  if (status === "done") {
    return (
      <span className="inline-flex items-center gap-1 text-muted-foreground">
        <Check size={12} aria-hidden="true" />
        Done
      </span>
    );
  }
  return (
    <span className="text-muted-foreground">{status === "denied" ? "Denied" : "Not finished"}</span>
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
        <pre className="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">
          <Untrusted text={shown} />
        </pre>
      )}
      {capped && !showAll && (
        <button
          type="button"
          className="text-xs underline underline-offset-2"
          onClick={() => setShowAll(true)}
        >
          Show all
        </button>
      )}
      {truncated && <p className="text-xs text-muted-foreground">Result truncated.</p>}
    </div>
  );
}

function ToolDetails({ item }: { item: ToolItem }): JSX.Element {
  const showPatch = (item.name === "Edit" || item.name === "Write") && item.patch !== null;
  return (
    <div className="min-w-0 space-y-2 py-2">
      <div>
        <p className="mb-1 text-xs font-medium text-muted-foreground">Input</p>
        <pre className="whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">
          <Untrusted text={item.input} />
        </pre>
      </div>
      {showPatch && (
        <div>
          <p className="mb-1 text-xs font-medium text-muted-foreground">Patch</p>
          <ToolOutput text={item.patch!} patch />
        </div>
      )}
      {item.result !== null && (
        <div>
          <p className="mb-1 text-xs font-medium text-muted-foreground">Result</p>
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
    <p
      className={`whitespace-pre-wrap break-words px-2 pb-1 text-xs ${item.status === "error" ? "text-(--tone-blocked)" : "text-muted-foreground"}`}
    >
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
  return (
    <button
      type="button"
      aria-expanded={expanded}
      aria-controls={detailsId}
      className="flex w-full min-w-0 items-start gap-2 rounded px-1 py-1 text-left hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring"
      onClick={onToggle}
    >
      <ChevronRight
        size={14}
        aria-hidden="true"
        className={`mt-0.5 shrink-0 ${expanded ? "rotate-90" : ""}`}
      />
      <Wrench size={14} aria-hidden="true" className="mt-0.5 shrink-0 text-muted-foreground" />
      <span className="flex min-w-0 flex-1 items-baseline gap-2">
        <span className="shrink-0 font-medium">
          <Untrusted text={(item.status === "done" && DONE_VERB[item.name]) || item.name} />
        </span>{" "}
        <span className="chat-tool-target">
          <Untrusted text={item.summary} />
        </span>
        {item.name === "Agent" && (
          <span className="shrink-0 text-xs text-muted-foreground">
            {item.nested_tools} tool {item.nested_tools === 1 ? "call" : "calls"}
          </span>
        )}
      </span>
      <span className="shrink-0 text-xs">
        <ToolStatus status={item.status} />
      </span>
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
    <div className="min-w-0 text-[12px] text-muted-foreground" aria-live="off">
      <ToolHeader
        item={item}
        expanded={expanded}
        detailsId={detailsId}
        onToggle={() => setExpanded((value) => !value)}
      />
      <ToolNote item={item} />
      <div id={detailsId} hidden={!expanded} className="min-w-0 px-2">
        {expanded && <ToolDetails key={item.id} item={item} />}
        {expanded && children.length > 0 && (
          <div className="min-w-0 pl-4">
            {children.map((child) => (
              <ToolRow key={child.id} item={child} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
