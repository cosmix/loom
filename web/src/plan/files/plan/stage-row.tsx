import { ChevronRightIcon } from "lucide-react";
import { useState } from "react";

import type { StageRow as StageRowData } from "@/plan/api/schema-files";

function TextList({ label, items }: { label: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div>
      <div className="text-xs font-medium text-muted-foreground">{label}</div>
      <ul aria-label={label} className="list-disc space-y-0.5 pl-5">
        {items.map((item, index) => (
          <li key={`${index}:${item}`}>{item}</li>
        ))}
      </ul>
    </div>
  );
}

/// One stage: id, name, type, summary and dependencies, expanding to its
/// description, acceptance criteria and files, all as plain text.
export function StageRow({ stage }: { stage: StageRowData }) {
  const [open, setOpen] = useState(false);
  const bodyId = `stage-body:${stage.id}`;
  return (
    <li id={`stage:${stage.id}`} className="scroll-mt-4 border-b border-(--hairline) py-2">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={() => setOpen((value) => !value)}
        className="flex w-full items-start gap-2 text-left"
      >
        <ChevronRightIcon
          aria-hidden="true"
          className={`mt-0.5 size-3.5 shrink-0 transition-transform ${open ? "rotate-90" : ""}`}
        />
        <span className="min-w-0 flex-1">
          <span className="flex flex-wrap items-baseline gap-x-2">
            <span className="font-mono text-xs">{stage.id}</span>
            <span className="text-[13px] font-medium">{stage.name}</span>
            <span className="text-xs text-muted-foreground">{stage.stage_type}</span>
          </span>
          {stage.summary !== null && (
            <span className="block text-[13px] text-muted-foreground">{stage.summary}</span>
          )}
          {stage.dependencies.length > 0 && (
            <span className="block text-xs text-muted-foreground">
              Depends on <span className="font-mono">{stage.dependencies.join(", ")}</span>
            </span>
          )}
        </span>
      </button>
      {open && (
        <div id={bodyId} className="mt-2 space-y-2 pl-5 text-[13px]">
          {stage.description !== "" && <p className="whitespace-pre-wrap">{stage.description}</p>}
          <TextList label="Acceptance" items={stage.acceptance} />
          <TextList label="Files" items={stage.files} />
        </div>
      )}
    </li>
  );
}
