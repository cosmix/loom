import { useAtomValue } from "jotai/react";
import { useEffect, useId, useRef, useState, type ComponentProps, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { apiGet } from "@/plan/api/client";
import type { AnyAttachmentInput } from "@/plan/api/schema-chat";
import { docViewSchema, type DocView, type TreeEntry } from "@/plan/api/schema-files";
import { docAttachment } from "@/plan/attachments/doc-attachment";
import { rememberPreview } from "@/plan/attachments/intake";
import {
  UPLOAD_ACCEPT,
  uploadAttachment,
  uploadProblem,
  type UploadedFile,
} from "@/plan/attachments/upload";
import { toAttachmentInput } from "@/plan/chat/commands";
import { Untrusted } from "@/plan/chat/visible-controls";
import { docRel } from "@/plan/files/address";
import { treeAtom } from "@/plan/files/atoms";
import { treeDocs } from "@/plan/files/tree";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import type { ButtonLook } from "@/plan/ui/disabled-reason";

/** `file` is set for an upload: the attachment then names it by id and the chip needs its size. */
export type StarterDocument = {
  attachment: AnyAttachmentInput;
  label: string;
  file?: UploadedFile;
};

function starterFile(file: UploadedFile): StarterDocument {
  return { label: file.name, attachment: { kind: "file", id: file.id, name: file.name }, file };
}

function starterDocument(doc: DocView): StarterDocument {
  return { label: doc.title, attachment: toAttachmentInput(docAttachment(doc)) };
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

/**
 * Uploads the files the user chose, one at a time, and hands each stored file to `onPick`. A
 * refused file is named with its reason and the rest still go. `room` caps how many are sent.
 */
function useFilePick(onPick: (document: StarterDocument) => void, room: number | undefined) {
  const input = useRef<HTMLInputElement>(null);
  const [uploading, setUploading] = useState(false);
  const [problems, setProblems] = useState<string[]>([]);
  const upload = async (files: File[], done: () => void) => {
    setUploading(true);
    setProblems([]);
    const refused: string[] = [];
    let stored = 0;
    for (const [index, file] of files.entries()) {
      const problem =
        room !== undefined && index >= room
          ? `${file.name} was not attached: at most 10 attachments per message.`
          : uploadProblem(file);
      if (problem !== null) {
        refused.push(problem);
        continue;
      }
      try {
        const uploaded = await uploadAttachment(file);
        rememberPreview(uploaded.id, file);
        onPick(starterFile(uploaded));
        stored += 1;
      } catch (error) {
        refused.push(`${file.name}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
    setUploading(false);
    setProblems(refused);
    if (refused.length === 0 && stored > 0) done();
  };
  return { input, uploading, problems, upload, clear: () => setProblems([]) };
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

type StarterMenuProps = {
  documents: TreeEntry[];
  pick: ReturnType<typeof useDocumentPick>;
  files: ReturnType<typeof useFilePick>;
};

/** The menu's two choices: a file from this computer, or a document in this project. */
function StarterMenu({ documents, pick, files }: StarterMenuProps) {
  const { pending, error } = pick;
  return (
    <>
      <button
        type="button"
        className="w-full rounded-md px-2 py-2 text-left font-medium hover:bg-muted focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50"
        disabled={files.uploading}
        onClick={() => files.input.current?.click()}
      >
        A file from this computer…
      </button>
      {files.uploading && <p role="status">Uploading…</p>}
      {files.problems.map((problem, index) => (
        <p key={index} role="alert" className="text-destructive">
          <Untrusted text={problem} />
        </p>
      ))}
      <p className="px-2 pt-2 text-muted-foreground">A document in this project</p>
      <ul aria-label="Documents" className="max-h-72 overflow-y-auto" aria-busy={pending}>
        {documents.map((entry) => (
          <DocumentOption key={entry.path} entry={entry} pending={pending} onPick={pick.pick} />
        ))}
      </ul>
      {documents.length === 0 && <p className="text-muted-foreground">No documents available.</p>}
      {pending && <p role="status">Loading document…</p>}
      {error !== null && (
        <p role="alert" className="text-destructive">
          Could not load the document: <Untrusted text={error} />
        </p>
      )}
    </>
  );
}

export function DocumentStarter({
  onPick,
  children = "Start from a document",
  className,
  variant = "outline",
  look,
  disabled,
  room,
}: {
  onPick: (document: StarterDocument) => void;
  /** The trigger's content; the default names the picker. */
  children?: ReactNode;
  className?: string;
  variant?: ComponentProps<typeof Button>["variant"];
  /** Draws the trigger on the planner's button scale; without it the caller's class styles it. */
  look?: ButtonLook;
  disabled?: boolean;
  /** How many more attachments fit; files past it are refused rather than uploaded. */
  room?: number;
}) {
  const tree = useAtomValue(treeAtom);
  const pick = useDocumentPick(onPick);
  const files = useFilePick(onPick, room);
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
    <>
      <Popover
        open={pick.open}
        onOpenChange={(next) => {
          if (!next) files.clear();
          pick.changeOpen(next);
        }}
      >
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
          <StarterMenu documents={documents} pick={pick} files={files} />
        </PopoverContent>
      </Popover>
      {/* Outside the popover, so an upload finishes even if the menu is dismissed. */}
      <input
        ref={files.input}
        type="file"
        multiple
        hidden
        accept={UPLOAD_ACCEPT}
        onChange={(event) => {
          const chosen = Array.from(event.target.files ?? []);
          event.target.value = "";
          if (chosen.length > 0) void files.upload(chosen, () => pick.changeOpen(false));
        }}
      />
    </>
  );
}
