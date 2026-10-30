import { atom, useAtomValue, useStore } from "jotai";
import { History } from "lucide-react";
import { useEffect, useRef, useState, type ComponentProps } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import "@/plan/header/activity.css";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DiffView } from "@/plan/diff/diff-view";
import { fetchDoc } from "@/plan/files/api";
import { planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";
import { ActivityTile } from "@/plan/ui/activity-glyphs";
import { LoadState } from "@/plan/ui/load-state";

import { getAtCommit, getCompare, type CompareResponse } from "./api";
import { changedSince, checkpointAtom, markSeen, type Checkpoint } from "./checkpoint";
import { ChangeRows, Limits, RefForm, type Change, type Comparison } from "./changed-since-parts";
import { focusReadingRegion } from "./focus-return";
import { hashesAtom, loadHashes } from "./live";

type Selection = { path: string; commit: string; checkpoint: Checkpoint | null };
type CompareLoad = { value?: CompareResponse; error?: string; loading: boolean };
type DiffLoad = { before: string | null; after: string | null } | { error: string } | null;

// One computation shared by every tree row, key chip and the header control.
const changesAtom = atom((get) => {
  const checkpoint = get(checkpointAtom);
  const current = get(hashesAtom);
  return checkpoint && current ? changedSince(checkpoint, current.docs) : [];
});

export function useChangedSince(): Change[] {
  const store = useStore();
  const current = useAtomValue(hashesAtom);
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  useEffect(() => {
    if (projectKey && current && store.get(checkpointAtom) === null) {
      store.set(checkpointAtom, { commit: current.head, hashes: { ...current.docs } });
    }
  }, [store, current, projectKey]);
  return useAtomValue(changesAtom);
}

function useCompare(commit: string | null) {
  const socket = useAtomValue(planSocketAtom);
  const hashes = useAtomValue(hashesAtom);
  const [attempt, setAttempt] = useState(0);
  const [load, setLoad] = useState<CompareLoad>({ loading: true });
  useEffect(() => socket?.onResync(() => setAttempt((n) => n + 1)), [socket]);
  useEffect(() => {
    let active = true;
    if (commit === null) {
      setLoad({ loading: false });
      return;
    }
    setLoad({ loading: true });
    void getCompare(commit).then((result) => {
      if (!active) return;
      setLoad(
        result.ok
          ? { loading: false, value: result.value }
          : { loading: false, error: result.message },
      );
    });
    return () => {
      active = false;
    };
  }, [commit, hashes, attempt]);
  return { ...load, retry: () => setAttempt((n) => n + 1) };
}

function acknowledge(
  store: ReturnType<typeof useStore>,
  selection: Selection,
  hash: string | null,
) {
  if (!selection.checkpoint || store.get(checkpointAtom) !== selection.checkpoint) return;
  if (hash !== null) {
    markSeen(store, selection.path, hash);
  } else {
    // markSeen accepts a hash; a reviewed deletion removes the old hash instead.
    const hashes = { ...selection.checkpoint.hashes };
    delete hashes[selection.path];
    store.set(checkpointAtom, { ...selection.checkpoint, hashes });
  }
}

function useSelectedDiff(selection: Selection) {
  const store = useStore();
  const [load, setLoad] = useState<DiffLoad>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let active = true;
    const projectKey = store.get(serverInfoAtom)?.project_key;
    setLoad(null);
    void Promise.all([
      getAtCommit(selection.commit, selection.path),
      fetchDoc(selection.path),
    ]).then(([before, after]) => {
      if (!active || store.get(serverInfoAtom)?.project_key !== projectKey) return;
      if (!before.ok) return setLoad({ error: before.message });
      if (!after.ok && after.status !== 404) return setLoad({ error: after.message });
      if (after.ok && (after.value.too_large || after.value.truncated)) {
        return setLoad({ error: "This document is too large to show a complete diff." });
      }
      setLoad({ before: before.value.text, after: after.ok ? after.value.text : null });
      acknowledge(store, selection, after.ok ? after.value.hash : null);
    });
    return () => {
      active = false;
    };
  }, [store, selection, attempt]);
  return { load, retry: () => setAttempt((n) => n + 1) };
}

function SelectedDiff({ selection }: { selection: Selection }) {
  const { load, retry } = useSelectedDiff(selection);
  if (!load) return <LoadState state="loading" title="Loading the diff" />;
  if ("error" in load)
    return (
      <LoadState
        state="error"
        message={visibleControls(load.error)}
        action={{ label: "Retry diff", onClick: retry }}
      />
    );
  return (
    <div className="flex min-h-64 flex-col border-t border-hairline pt-3">
      <p className="mb-2 font-mono text-[12px]">
        <Untrusted text={selection.path} />
      </p>
      <DiffView path={selection.path} before={load.before} after={load.after} />
    </div>
  );
}

type MarkState = { pending: boolean; error: string | null };
type MarkAll = MarkState & { markAll: () => void };

/**
 * Stores what the server holds when the button is pressed. `hashesAtom` reloads on load, reconnect
 * and `doc_changed`, and a commit sends none of them, so its `head` can predate the commit HEAD
 * points to now; the checkpoint must hold that commit. The load goes through the serial queue of
 * `loadHashes`, which also replaces `hashesAtom` with the response the checkpoint is cut from; a
 * `doc_changed` reload that starts meanwhile queues behind it, so an older response never lands
 * after a newer one.
 */
function useMarkAllSeen(): MarkAll {
  const store = useStore();
  const [state, setState] = useState<MarkState>({ pending: false, error: null });
  const mounted = useRef(true);
  const busy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const markAll = () => {
    if (busy.current) return;
    busy.current = true;
    setState({ pending: true, error: null });
    void loadHashes(store).then((result) => {
      busy.current = false;
      if (!mounted.current) return;
      if (result.ok) {
        store.set(checkpointAtom, { commit: result.value.head, hashes: { ...result.value.docs } });
      }
      setState({ pending: false, error: result.ok ? null : result.message });
    });
  };
  return { ...state, markAll };
}

function CompareControls({
  comparison,
  mark,
  onRef,
}: {
  comparison: Comparison | null;
  mark: MarkAll;
  onRef: (value: Comparison | null) => void;
}) {
  return (
    <>
      <RefForm onCompare={onRef} />
      {comparison && (
        <p>
          Comparing with <Untrusted text={comparison.name} />. This ref is not stored.
        </p>
      )}
      {!comparison && (
        // `aria-disabled`, not `disabled`: the button holds focus inside the modal while it loads, and
        // a disabled button drops it to the page body. `markAll` ignores presses while pending.
        <Button
          size="sm"
          variant="outline"
          className="aria-disabled:cursor-not-allowed aria-disabled:opacity-50"
          aria-disabled={mark.pending || undefined}
          onClick={mark.markAll}
        >
          {mark.pending ? "Marking all seen…" : "Mark all seen"}
        </Button>
      )}
      {mark.error && (
        <LoadState
          state="error"
          title="Could not mark all seen"
          message={visibleControls(mark.error)}
        />
      )}
    </>
  );
}

function CompareStatus({
  compare,
  unavailable,
}: {
  compare: ReturnType<typeof useCompare>;
  unavailable: boolean;
}) {
  return (
    <>
      {compare.loading && <LoadState state="loading" title="Loading comparison" />}
      {compare.error && (
        <LoadState
          state="error"
          message={visibleControls(compare.error)}
          action={{ label: "Retry comparison", onClick: compare.retry }}
        />
      )}
      {unavailable && (
        <p role="status">
          The checkpoint commit is unavailable. Only uncommitted and ref diffs are available.
        </p>
      )}
    </>
  );
}

function ComparisonList({ changes }: { changes: Change[] }) {
  const checkpoint = useAtomValue(checkpointAtom);
  const mark = useMarkAllSeen();
  const [ref, setRef] = useState<Comparison | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const commit = ref ? ref.commit : (checkpoint?.commit ?? null);
  const compare = useCompare(commit);
  const available = commit !== null && compare.value?.available === true && !compare.loading;
  const unavailable = commit === null || compare.value?.available === false;
  const rows = ref ? (compare.value?.docs ?? []) : changes;
  const shown = unavailable ? rows.map((row): Change => ({ ...row, change: "changed" })) : rows;
  return (
    <>
      <CompareControls
        comparison={ref}
        mark={mark}
        onRef={(value) => {
          setRef(value);
          setSelection(null);
        }}
      />
      <CompareStatus compare={compare} unavailable={unavailable} />
      <ChangeRows
        rows={shown}
        available={available}
        checkpointMode={!ref}
        onOpen={(path) => {
          if (commit) setSelection({ path, commit, checkpoint: ref ? null : checkpoint });
        }}
      />
      <Limits />
      {selection && (
        <SelectedDiff key={`${selection.commit}:${selection.path}`} selection={selection} />
      )}
    </>
  );
}

type Variant = "button" | "row";

/**
 * The footer row of the activity popover: a tile, the title and the count, in one button. It
 * spreads the props DialogTrigger hands its child (handlers, ARIA state and the ref).
 */
function RowTrigger({ count, ...props }: ComponentProps<"button"> & { count: number }) {
  return (
    <button
      type="button"
      className="activity-row"
      aria-label={`Changed since you looked · ${count}`}
      {...props}
    >
      <ActivityTile>
        <History />
      </ActivityTile>
      <span className="activity-row__body">
        <span className="activity-row__title">Changed since you looked</span>{" "}
        <span className="activity-row__sub">Review against your checkpoint or a git ref</span>
      </span>{" "}
      {/* The spaces keep the row's text readable ("… ref 2"); grid and flex layout ignore them. */}
      <span className="activity-row__time">{count}</span>
    </button>
  );
}

function ChangedSinceWidget({
  compact,
  variant,
  changes,
}: {
  compact: boolean;
  variant: Variant;
  changes: Change[];
}) {
  const [open, setOpen] = useState(false);
  const opener = useRef<HTMLButtonElement>(null);
  if (!changes.length && !open) return null;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {changes.length > 0 && (
        <DialogTrigger asChild>
          {variant === "row" ? (
            <RowTrigger ref={opener} count={changes.length} />
          ) : (
            <Button
              ref={opener}
              variant="ghost"
              size="sm"
              aria-label={`Changed since you looked · ${changes.length}`}
            >
              {compact ? (
                <>
                  <span aria-hidden="true">●</span> {changes.length}
                </>
              ) : (
                <>Changed since you looked · {changes.length}</>
              )}
            </Button>
          )}
        </DialogTrigger>
      )}
      <DialogContent
        className="max-h-[85vh] overflow-y-auto text-[13px] motion-reduce:animate-none sm:max-w-3xl"
        onCloseAutoFocus={(event) => {
          // "Mark all seen" removes the opener; Radix would leave focus on the page body.
          if (opener.current?.isConnected) return;
          event.preventDefault();
          focusReadingRegion();
        }}
      >
        <DialogTitle>Changed since you looked</DialogTitle>
        <DialogDescription>
          Review documents against your checkpoint or a git ref.
        </DialogDescription>
        {open && <ComparisonList changes={changes} />}
      </DialogContent>
    </Dialog>
  );
}

/** `row` is the activity popover's footer row, which has room for the full title and ignores `compact`. */
export function ChangedSinceControl({
  compact = false,
  variant = "button",
}: {
  compact?: boolean;
  variant?: Variant;
}) {
  const changes = useChangedSince();
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  return projectKey ? (
    <ChangedSinceWidget key={projectKey} compact={compact} variant={variant} changes={changes} />
  ) : null;
}
