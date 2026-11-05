import { useAtomValue, useStore } from "jotai";
import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import type { Turn } from "@/plan/api/schema-chat";
import { requestEdit } from "@/plan/editor/request-edit";
import { docRel } from "@/plan/files/address";
import { fetchTurnDiff } from "@/plan/files/api";
import { docChangedAtom } from "@/plan/files/atoms";
import { postRestore, type RestoreResponse } from "@/plan/review/api";
import { authorBlockReasonAtom, connectionEpochAtom, isHolderAtom } from "@/plan/state/atoms";
import { BusyText } from "@/plan/ui/busy-text";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import { snapshotAtom } from "./model";
import type { TurnRow } from "./use-turn-texts";
import { Untrusted } from "./visible-controls";

/** `texts` are the footer's already-fetched turn texts; without them Restore fetches its own. */
type Props = { sessionId: string; turn: Turn; paths: string[]; texts?: TurnRow[] };
type FileResult = RestoreResponse["files"][number];
type Invalid = Extract<FileResult, { result: "invalid" }>;
type ModifiedFile = { path: string; after: string | undefined };

function useAvailable(turn: Turn): boolean {
  const holder = useAtomValue(isHolderAtom);
  const model = useAtomValue(snapshotAtom);
  const session = model.session;
  const running =
    session &&
    (["starting", "working", "permission", "ending"].includes(session.state) ||
      session.subagents_running > 0);
  return holder && !turn.open && !running && !model.turns.some((row) => row.open);
}

/** Restorable files from the footer's shared texts: no row yet means the fetch is in flight. */
function filesFromRows(candidates: string[], rows: TurnRow[]): ModifiedFile[] {
  const byPath = new Map(rows.map((row) => [row.path, row]));
  return candidates.flatMap((path): ModifiedFile[] => {
    const row = byPath.get(path);
    if (!row) return [];
    // Missing pins are also checked by Restore; let it explain a refusal inline.
    if (!row.texts) return [{ path, after: undefined }];
    if (row.texts.before === null || row.texts.after === null) return [];
    return [{ path, after: row.texts.after }];
  });
}

/** Fallback for a caller that passes no shared texts: fetch the candidates itself. */
function useFetchedFiles(
  { sessionId, turn }: Props,
  candidates: string[],
  enabled: boolean,
): ModifiedFile[] {
  const epoch = useAtomValue(connectionEpochAtom);
  const identity = `${sessionId}/${turn.id}/${epoch}/${candidates.join("\n")}`;
  const [loaded, setLoaded] = useState<{ identity: string; files: ModifiedFile[] } | null>(null);
  useEffect(() => {
    if (!enabled) return;
    let active = true;
    void Promise.all(
      candidates.map(async (path): Promise<ModifiedFile | null> => {
        const result = await fetchTurnDiff(`${sessionId}/${turn.id}`, docRel(path));
        if (result.ok) {
          if (result.value.before === null || result.value.after === null) return null;
          return { path, after: result.value.after };
        }
        return { path, after: undefined };
      }),
    ).then((files) => {
      if (active) setLoaded({ identity, files: files.filter((file) => file !== null) });
    });
    return () => {
      active = false;
    };
  }, [enabled, candidates, identity, sessionId, turn.id]);
  return loaded?.identity === identity ? loaded.files : [];
}

function useModifiedFiles(props: Props, available: boolean) {
  const { turn, paths, texts } = props;
  const changes = useAtomValue(docChangedAtom);
  const key = JSON.stringify(
    paths.filter(
      (path) =>
        path.startsWith("doc/") &&
        turn.changed_files.some((file) => file.path === path && file.before_hash !== null),
    ),
  );
  const candidates = useMemo(() => JSON.parse(key) as string[], [key]);
  const fetched = useFetchedFiles(props, candidates, available && !texts);
  const shared = useMemo(
    () => (texts ? filesFromRows(candidates, texts) : []),
    [candidates, texts],
  );
  return (texts ? shared : fetched).filter((file) => changes[file.path] !== null);
}

/** Matches planning::sections::normalized_hash, including the final newline state. */
async function endHash(text: string): Promise<string> {
  const normalized = text
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.replace(/[ \t]+$/g, ""))
    .join("\n");
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(normalized));
  return `sha256:${Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

async function pinnedAfter(sessionId: string, turn: Turn, file: ModifiedFile): Promise<string> {
  if (file.after !== undefined) return file.after;
  const diff = await fetchTurnDiff(`${sessionId}/${turn.id}`, docRel(file.path));
  if (!diff.ok) throw new Error(diff.message);
  if (diff.value.after === null) throw new Error("The turn deleted this file; ask in chat");
  return diff.value.after;
}

function useMerge(props: Props, files: ModifiedFile[], setError: (error: string) => void) {
  const store = useStore();
  const navigate = useNavigate();
  const [changed, setChanged] = useState<Extract<FileResult, { result: "changed" }>[]>([]);
  async function merge(file: Extract<FileResult, { result: "changed" }>) {
    const pinned = files.find((candidate) => candidate.path === file.path);
    if (!pinned) return;
    const after = await pinnedAfter(props.sessionId, props.turn, pinned);
    const openedHash = await endHash(after);
    void navigate(
      requestEdit(store, { path: docRel(file.path), text: file.turn_start_text, openedHash }),
    );
  }
  async function applyChanges(results: FileResult[]) {
    // requestEdit holds one edit: keep additional conflicts available as explicit actions.
    const conflicts = results.filter((file) => file.result === "changed");
    setChanged(conflicts);
    if (conflicts.length === 1) await merge(conflicts[0]);
  }
  const openMerge = (file: Extract<FileResult, { result: "changed" }>) => {
    void merge(file).catch((cause: unknown) =>
      setError(cause instanceof Error ? cause.message : String(cause)),
    );
  };
  return { changed, applyChanges, openMerge };
}

function useRestore(props: Props, files: ModifiedFile[], available: boolean) {
  const store = useStore();
  const sending = useRef(false);
  const [pending, setPending] = useState(false);
  const [invalid, setInvalid] = useState<Invalid[]>([]);
  const [notes, setNotes] = useState<string[]>([]);
  const [error, setError] = useState<string | null>(null);
  const { changed, applyChanges, openMerge } = useMerge(props, files, setError);
  const block = useAtomValue(authorBlockReasonAtom);
  const reason = block ?? (pending ? <BusyText>Restoring…</BusyText> : null);
  async function apply(results: FileResult[]) {
    setInvalid(results.filter((file): file is Invalid => file.result === "invalid"));
    setNotes(
      results.flatMap((file) => (file.result === "restored" ? [`Restored ${file.path}`] : [])),
    );
    const refused = results.flatMap((file) =>
      file.result === "refused" ? [`${file.path}: ${file.reason}`] : [],
    );
    if (refused.length) setError(refused.join("\n"));
    await applyChanges(results);
  }
  async function submit(paths: string[], force = false) {
    if (
      sending.current ||
      !available ||
      !store.get(isHolderAtom) ||
      store.get(authorBlockReasonAtom)
    )
      return;
    sending.current = true;
    setPending(true);
    setError(null);
    try {
      const result = await postRestore({
        session: props.sessionId,
        turn: props.turn.id,
        paths,
        force,
      });
      if (!result.ok) setError(result.message);
      else await apply(result.value.files.filter((file) => paths.includes(file.path)));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      sending.current = false;
      setPending(false);
    }
  }
  return { pending, reason, invalid, setInvalid, notes, error, changed, openMerge, submit };
}

type Action = ReturnType<typeof useRestore>;

function FileList({ paths }: { paths: string[] }) {
  return (
    <ul className="max-h-48 overflow-auto text-[13px] font-mono">
      {paths.map((path) => (
        <li key={path} className="break-all">
          <Untrusted text={path} />
        </li>
      ))}
    </ul>
  );
}

function ValidationDialog({ action }: { action: Action }) {
  const returnFocus = useRef<HTMLElement | null>(null);
  return (
    <Dialog
      open={action.invalid.length > 0}
      onOpenChange={(open) => {
        if (!open) action.setInvalid([]);
      }}
    >
      <DialogContent
        className="text-[13px] motion-reduce:animate-none"
        onOpenAutoFocus={() => {
          returnFocus.current = document.activeElement as HTMLElement | null;
        }}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocus.current?.focus();
        }}
      >
        <DialogTitle>Save with errors?</DialogTitle>
        <DialogDescription>The restored text has validation errors.</DialogDescription>
        <FileList paths={action.invalid.map((file) => file.path)} />
        <ul className="max-h-48 overflow-auto">
          {action.invalid.flatMap((file) =>
            file.errors.map((error, index) => (
              <li key={`${file.path}/${index}`}>
                <Untrusted text={error} />
              </li>
            )),
          )}
        </ul>
        <Button type="button" variant="outline" onClick={() => action.setInvalid([])}>
          Cancel
        </Button>
        <DisabledReason
          reason={action.reason}
          onClick={() => {
            const paths = action.invalid.map((file) => file.path);
            action.setInvalid([]);
            void action.submit(paths, true);
          }}
        >
          Save with errors
        </DisabledReason>
      </DialogContent>
    </Dialog>
  );
}

function Feedback({ action }: { action: Action }) {
  return (
    <>
      <span role="status">
        {action.notes.map((note) => (
          <span key={note}>
            <Untrusted text={note} />
          </span>
        ))}
      </span>
      {action.error && (
        <span role="alert" className="whitespace-pre-wrap">
          <Untrusted text={action.error} />
        </span>
      )}
      {action.changed.length > 1 &&
        action.changed.map((file) => (
          <Button
            key={file.path}
            type="button"
            variant="link"
            onClick={() => action.openMerge(file)}
          >
            Merge <Untrusted text={file.path} />
          </Button>
        ))}
      <ValidationDialog action={action} />
    </>
  );
}

function RestoreAllDialog({ action, paths }: { action: Action; paths: string[] }) {
  const [confirm, setConfirm] = useState(false);
  // The trigger stays focusable while a restore runs (`aria-disabled`, opening refused): confirming
  // starts the restore at once, and Radix hands focus back to the trigger as the dialog closes. A
  // `disabled` button cannot take it, and focus would fall to the page body.
  return (
    <Dialog open={confirm} onOpenChange={(open) => setConfirm(open && !action.pending)}>
      <DialogTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="aria-disabled:cursor-not-allowed aria-disabled:opacity-50"
          aria-disabled={action.pending || undefined}
        >
          Restore all
        </Button>
      </DialogTrigger>
      <DialogContent className="text-[13px] motion-reduce:animate-none">
        <DialogTitle>Restore all files?</DialogTitle>
        <DialogDescription>Restore these files to their text before this turn.</DialogDescription>
        <FileList paths={paths} />
        <Button type="button" variant="outline" onClick={() => setConfirm(false)}>
          Cancel
        </Button>
        <DisabledReason
          reason={action.reason}
          onClick={() => {
            setConfirm(false);
            void action.submit(paths);
          }}
        >
          Restore files
        </DisabledReason>
      </DialogContent>
    </Dialog>
  );
}

function RestoreControl(props: Props & { all: boolean }) {
  const available = useAvailable(props.turn);
  const files = useModifiedFiles(props, available);
  const action = useRestore(props, files, available);
  if (!available || files.length < (props.all ? 2 : 1)) return null;
  const paths = files.map((file) => file.path);
  return (
    <div className="inline-flex flex-wrap items-center gap-2 text-[13px]">
      {props.all ? (
        <RestoreAllDialog action={action} paths={paths} />
      ) : (
        <DisabledReason
          size="sm"
          reason={action.reason}
          onClick={() => {
            void action.submit(paths);
          }}
        >
          Restore
        </DisabledReason>
      )}
      <Feedback action={action} />
    </div>
  );
}

export function RestoreButton({
  sessionId,
  turn,
  path,
  texts,
}: {
  sessionId: string;
  turn: Turn;
  path: string;
  texts?: TurnRow[];
}) {
  const paths = useMemo(() => [path], [path]);
  return (
    <RestoreControl sessionId={sessionId} turn={turn} paths={paths} texts={texts} all={false} />
  );
}

export function RestoreAll(props: Props) {
  return <RestoreControl {...props} all />;
}
