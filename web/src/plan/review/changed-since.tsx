import { atom, useAtomValue, useStore } from "jotai";
import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DiffView } from "@/plan/diff/diff-view";
import { fetchDoc } from "@/plan/files/api";
import { planSocketAtom, serverInfoAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";

import { getAtCommit, getCompare, type CompareResponse } from "./api";
import { changedSince, checkpointAtom, markSeen, type Checkpoint } from "./checkpoint";
import { ChangeRows, Limits, RefForm, type Change, type Comparison } from "./changed-since-parts";
import { hashesAtom } from "./live";

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

function CompareControls({
  comparison,
  canMarkAll,
  onRef,
  onMarkAll,
}: {
  comparison: Comparison | null;
  canMarkAll: boolean;
  onRef: (value: Comparison | null) => void;
  onMarkAll: () => void;
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
        <Button size="sm" variant="outline" disabled={!canMarkAll} onClick={onMarkAll}>
          Mark all seen
        </Button>
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
  const store = useStore();
  const checkpoint = useAtomValue(checkpointAtom);
  const current = useAtomValue(hashesAtom);
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
        canMarkAll={!!current}
        onRef={(value) => {
          setRef(value);
          setSelection(null);
        }}
        onMarkAll={() => {
          if (!current) return;
          store.set(checkpointAtom, { commit: current.head, hashes: { ...current.docs } });
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

function ChangedSinceWidget({ compact, changes }: { compact: boolean; changes: Change[] }) {
  const [open, setOpen] = useState(false);
  if (!changes.length && !open) return null;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {changes.length > 0 && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => setOpen(true)}
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
      <DialogContent className="max-h-[85vh] overflow-y-auto text-[13px] motion-reduce:animate-none sm:max-w-3xl">
        <DialogTitle>Changed since you looked</DialogTitle>
        <DialogDescription>
          Review documents against your checkpoint or a git ref.
        </DialogDescription>
        {open && <ComparisonList changes={changes} />}
      </DialogContent>
    </Dialog>
  );
}

export function ChangedSinceControl({ compact }: { compact: boolean }) {
  const changes = useChangedSince();
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  return projectKey ? (
    <ChangedSinceWidget key={projectKey} compact={compact} changes={changes} />
  ) : null;
}
