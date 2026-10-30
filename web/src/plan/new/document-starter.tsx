import { useAtomValue } from "jotai/react";
import { useEffect, useId, useRef, useState, type ComponentProps, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { apiGet } from "@/plan/api/client";
import type { AttachmentInput } from "@/plan/api/schema-chat";
import { docViewSchema, type DocView, type TreeEntry } from "@/plan/api/schema-files";
import { toAttachmentInput } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { docRel } from "@/plan/files/address";
import { treeAtom } from "@/plan/files/atoms";
import { treeDocs } from "@/plan/files/tree";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import type { ButtonLook } from "@/plan/ui/disabled-reason";

export type StarterDocument = { attachment: AttachmentInput; label: string };

// The matching QUOTE_CHARS in chat/format-attachments.ts is private to that module.
const QUOTE_CHARS = 2_000;

function starterDocument(doc: DocView): StarterDocument {
  return {
    label: doc.title,
    attachment: toAttachmentInput({
      kind: "source",
      path: doc.path,
      anchor: null,
      lines: null,
      stageId: null,
      text: Array.from(doc.text).slice(0, QUOTE_CHARS).join(""),
      hash: doc.hash,
    }),
  };
}

function useDocumentPick(onPick: (document: StarterDocument) => void) {
  const [open, setOpen] = useState(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef<AbortController | null>(null);
  useEffect(() => () => request.current?.abort(), []);

  const changeOpen = (next: boolean) => {
    request.current?.abort();
    request.current = null;
    setPending(false);
    setError(null);
    setOpen(next);
  };
  const pick = async (entry: TreeEntry) => {
    if (!entry.path || request.current) return;
    const controller = new AbortController();
    request.current = controller;
    setPending(true);
    setError(null);
    const result = await apiGet(
      `/api/doc?path=${encodeURIComponent(docRel(entry.path))}`,
      docViewSchema,
      controller.signal,
    );
    if (controller.signal.aborted) return;
    request.current = null;
    setPending(false);
    if (!result.ok) {
      setError(result.message);
      return;
    }
    changeOpen(false);
    onPick(starterDocument(result.value));
  };
  return { open, changeOpen, pending, error, pick };
}

function DocumentOption({
  entry,
  pending,
  onPick,
}: {
  entry: TreeEntry;
  pending: boolean;
  onPick: (entry: TreeEntry) => Promise<void>;
}) {
  const id = useId();
  return (
    <li>
      <button
        type="button"
        aria-labelledby={`${id}-title`}
        aria-describedby={`${id}-path`}
        disabled={pending}
        className="flex w-full flex-col gap-0.5 rounded-md px-2 py-2 text-left hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50"
        onClick={() => void onPick(entry)}
      >
        <span id={`${id}-title`} className="break-words font-medium">
          <Untrusted text={entry.title} />
        </span>
        <span id={`${id}-path`} className="break-all font-mono text-xs text-muted-foreground">
          <Untrusted text={entry.path ?? ""} />
        </span>
      </button>
    </li>
  );
}

export function DocumentStarter({
  onPick,
  children = "Start from a document",
  className,
  variant = "outline",
  look,
  disabled,
}: {
  onPick: (document: StarterDocument) => void;
  /** The trigger's content; the default names the picker. */
  children?: ReactNode;
  className?: string;
  variant?: ComponentProps<typeof Button>["variant"];
  /** Draws the trigger on the planner's button scale; without it the caller's class styles it. */
  look?: ButtonLook;
  disabled?: boolean;
}) {
  const tree = useAtomValue(treeAtom);
  const { open, changeOpen, pending, error, pick } = useDocumentPick(onPick);
  const documents = tree
    ? treeDocs(tree).filter(
        (entry) =>
          entry.path !== "" &&
          !entry.gap &&
          entry.kind !== "feature" &&
          !entry.kind.endsWith("_folder"),
      )
    : [];
  return (
    <Popover open={open} onOpenChange={changeOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant={variant}
          data-look={look}
          className={look ? `plan-button ${className ?? ""}` : className}
          disabled={disabled}
        >
          {children}
        </Button>
      </PopoverTrigger>
      <PopoverContent aria-label="Choose a starter document" align="start">
        <ul aria-label="Documents" className="max-h-72 overflow-y-auto" aria-busy={pending}>
          {documents.map((entry) => (
            <DocumentOption key={entry.path} entry={entry} pending={pending} onPick={pick} />
          ))}
        </ul>
        {documents.length === 0 && <p className="text-muted-foreground">No documents available.</p>}
        {pending && <p role="status">Loading document…</p>}
        {error !== null && (
          <p role="alert" className="text-destructive">
            Could not load the document: <Untrusted text={error} />
          </p>
        )}
      </PopoverContent>
    </Popover>
  );
}
