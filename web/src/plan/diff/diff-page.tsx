import { useAtomValue } from "jotai/react";
import { useCallback, useEffect, useState, type JSX } from "react";
import { Link } from "react-router";

import type { ApiResult } from "@/plan/api/client";
import type { DiffTexts } from "@/plan/api/schema-files";
import { Untrusted } from "@/plan/chat/visible-controls";
import { toAddress } from "@/plan/files/address";
import { fetchHeadDiff, fetchTurnDiff } from "@/plan/files/api";
import { openDocAtom, previousOpenTextAtom } from "@/plan/files/atoms";
import { LoadState } from "@/plan/ui/load-state";

import { DiffView } from "./diff-view";
import { openDiff } from "./open-diff";

export interface DiffPageProps {
  path: string;
  kind: "turn" | "open" | "head";
  /** The full turn id; only read for `kind: "turn"`. */
  turn?: string | null;
}

const KIND_LABEL = {
  turn: "Changes in a turn",
  open: "Changes while open",
  head: "Uncommitted changes",
};

type Load =
  | { status: "loading" }
  | { status: "ready"; diff: DiffTexts }
  | { status: "failed"; code: number; message: string };

/** Fetches a turn or head diff; `attempt` re-runs it for Retry. */
function useServerDiff(kind: "turn" | "head", path: string, turn: string | null | undefined) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let cancelled = false;
    setLoad({ status: "loading" });
    const request: Promise<ApiResult<DiffTexts>> =
      kind === "turn" ? fetchTurnDiff(turn ?? "", path) : fetchHeadDiff(path);
    void request.then((result) => {
      if (cancelled) return;
      setLoad(
        result.ok
          ? { status: "ready", diff: result.value }
          : { status: "failed", code: result.status, message: result.message },
      );
    });
    return () => {
      cancelled = true;
    };
  }, [kind, path, turn, attempt]);
  return { load, retry: useCallback(() => setAttempt((n) => n + 1), []) };
}

function Header({ kind, path }: { kind: DiffPageProps["kind"]; path: string }): JSX.Element {
  return (
    <header className="flex flex-wrap items-baseline gap-x-3 border-b border-hairline px-4 py-2 text-[13px]">
      <span className="font-medium">{KIND_LABEL[kind]}</span>
      <span className="font-mono text-muted-foreground">
        <Untrusted text={path} />
      </span>
      <Link to={toAddress(path)} className="underline">
        Back to the document
      </Link>
    </header>
  );
}

function NotFound({ kind, path }: { kind: "turn" | "head"; path: string }): JSX.Element {
  if (kind === "head") {
    return <LoadState state="not_found" title="This document no longer exists" />;
  }
  return (
    <LoadState
      state="not_found"
      title="This turn's diff is gone"
      message="This turn's diff is gone: loom plan restarted. The uncommitted diff still shows the work."
    >
      <Link to={openDiff("head", path)} className="underline">
        Show the uncommitted diff
      </Link>
    </LoadState>
  );
}

// Turn and head diffs attach to the chat with anchor null: the page has no sections to give
// either text (only the open-document diff passes them).
function ServerDiff({ kind, path, turn }: DiffPageProps & { kind: "turn" | "head" }): JSX.Element {
  const { load, retry } = useServerDiff(kind, path, turn);
  if (load.status === "loading") return <LoadState state="loading" title="Loading the diff" />;
  if (load.status === "failed") {
    if (load.code === 404) return <NotFound kind={kind} path={path} />;
    return (
      <LoadState
        state="error"
        title="The diff did not load"
        message={load.message}
        action={{ label: "Retry", onClick: retry }}
      />
    );
  }
  return <DiffView before={load.diff.before} after={load.diff.after} path={path} />;
}

function OpenDiff({ path }: { path: string }): JSX.Element {
  const previous = useAtomValue(previousOpenTextAtom);
  const open = useAtomValue(openDocAtom);
  const after = open && open.doc.path === path ? open.doc.text : null;
  if (!open || !previous || previous.path !== path || after === null) {
    return <LoadState state="empty" title="Nothing changed while open" />;
  }
  return <DiffView before={previous.text} after={after} path={path} sections={open.doc.sections} />;
}

function DiffBody({ path, kind, turn }: DiffPageProps): JSX.Element {
  if (kind === "open") return <OpenDiff path={path} />;
  // No turn id: the server would answer 400 and Retry could not change that, so do not ask.
  if (kind === "turn" && !turn) return <NotFound kind="turn" path={path} />;
  return <ServerDiff kind={kind} path={path} turn={turn} />;
}

/** The diff address of file mode: the header and the diff of one kind. */
export function DiffPage(props: DiffPageProps): JSX.Element {
  return (
    <div className="flex min-h-full flex-1 flex-col">
      <Header kind={props.kind} path={props.path} />
      <DiffBody {...props} />
    </div>
  );
}
