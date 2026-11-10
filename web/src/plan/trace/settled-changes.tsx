import { FileDiff } from "lucide-react";
import { useEffect, useId, useMemo, useRef, useState, type JSX, type RefObject } from "react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { apiGet, type ApiResult } from "@/plan/api/client";
import { settledVersionSchema, type SettledVersion } from "@/plan/api/schema-trace";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DiffView } from "@/plan/diff/diff-view";
import { docRel } from "@/plan/files/address";
import { featureOfDocPath, sourceHref } from "@/plan/feature/source-href";
import { HEADING_BUTTON } from "@/plan/ui/doc-heading";
import { useSelectedDiff } from "@/plan/review/changed-since-data";
import { shortSha } from "@/plan/review/changed-since-parts";
import { returnFocusTo } from "@/plan/review/focus-return";
import { LoadState } from "@/plan/ui/load-state";

import { docOfPath } from "./model";
import { useSettleCommits } from "./settle-commits";
import { useTrace } from "./state";

/** What asking for the settled version came to: not yet, a commit to compare, or why not. */
type Lookup =
  | { kind: "asking" }
  | { kind: "commit"; commit: string }
  | { kind: "uncommitted" }
  | { kind: "failed"; message: string };

/** Why a settle that was never committed has nothing to compare: in full beside the settle
 *  control, where settling again is at hand, and short inside a sentence. */
const NEVER_COMMITTED = {
  button:
    "The settled version was never committed, so there is nothing to compare. Settle again to record it.",
  inline: "Nothing to compare: the settled version was never committed.",
} as const;

function lookupOf(result: ApiResult<SettledVersion>): Lookup {
  if (!result.ok) return { kind: "failed", message: result.message };
  const { commit } = result.value;
  return commit === null ? { kind: "uncommitted" } : { kind: "commit", commit };
}

/**
 * Which commit holds `settled`'s Settled line, asked on mount and again when the trace shows the
 * document's settle state or text change (a settle or an edit can move it), when a settle commit
 * of the document ends (`settle_commit`: the commit runs after the settle answers), and on
 * `retry`. Only the latest request's answer shows: one overtaken by a later request is dropped
 * whenever it arrives, and an answer for another path never shows.
 */
function useSettledVersion(settled: string): { lookup: Lookup; retry: () => void } {
  const { trace } = useTrace(featureOfDocPath(settled) ?? "");
  const doc = trace ? docOfPath(trace, settled) : undefined;
  const stamp = doc ? `${doc.settle} ${doc.current.body}` : "";
  const [attempt, setAttempt] = useState(0);
  const [commits, setCommits] = useState(0);
  const [answer, setAnswer] = useState<{ path: string; lookup: Lookup } | null>(null);
  useSettleCommits(settled, () => setCommits((count) => count + 1));
  useEffect(() => {
    let ignore = false;
    const query = `/api/settled-version?path=${encodeURIComponent(docRel(settled))}`;
    void apiGet(query, settledVersionSchema).then((result) => {
      if (!ignore) setAnswer({ path: settled, lookup: lookupOf(result) });
    });
    return () => {
      ignore = true;
    };
  }, [settled, stamp, attempt, commits]);
  const lookup = answer?.path === settled ? answer.lookup : { kind: "asking" as const };
  const retry = () => {
    setAnswer(null);
    setAttempt((count) => count + 1);
  };
  return { lookup, retry };
}

/** Why See what changed does nothing now, or what a press does instead of comparing. */
function LookupReason(props: {
  lookup: Lookup;
  id: string;
  look: "button" | "inline";
}): JSX.Element | null {
  const { lookup, id, look } = props;
  if (lookup.kind === "uncommitted") {
    return (
      <span id={id} className="plan-fine text-muted-foreground trace-changes-reason">
        {NEVER_COMMITTED[look]}
      </span>
    );
  }
  if (lookup.kind !== "failed") return null;
  return (
    <span id={id} className="plan-fine text-muted-foreground trace-changes-reason">
      The settled version did not load: <Untrusted text={lookup.message} />. Select See what changed
      to try again.
    </span>
  );
}

/** The review module's commit comparison: the settled commit's text against the file now. */
function SettledDiff({ path, commit }: { path: string; commit: string }): JSX.Element {
  const selection = useMemo(
    () => ({ path, ref: { commit, name: "the settled version" } }),
    [path, commit],
  );
  const { load, retry } = useSelectedDiff(selection);
  if (!load) return <LoadState state="loading" size="compact" title="Loading the comparison" />;
  if ("error" in load) {
    return (
      <LoadState
        state="error"
        size="compact"
        title="The comparison did not load"
        message={visibleControls(load.error)}
        action={{ label: "Retry", onClick: retry }}
      />
    );
  }
  return <DiffView path={path} before={load.before} after={load.after} />;
}

function SettledDialog(props: {
  path: string;
  commit: string;
  link: RefObject<HTMLAnchorElement | null>;
  onClose: () => void;
}): JSX.Element {
  const { path, commit, link, onClose } = props;
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        className="flex max-h-[90vh] flex-col overflow-auto sm:max-w-5xl motion-reduce:animate-none"
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          returnFocusTo(link.current);
        }}
      >
        <DialogTitle>Since this document was settled</DialogTitle>
        <DialogDescription>
          From the settled version (commit <Untrusted text={shortSha(commit)} />) to the current
          file.
        </DialogDescription>
        <SettledDiff path={path} commit={commit} />
      </DialogContent>
    </Dialog>
  );
}

/**
 * "See what changed": a link to the commit comparison of `compare` (default: the same document)
 * against the commit holding `settled`'s current Settled line, which it asks for as it mounts. It
 * stays disabled while it asks; a line that was never committed has no version to compare with,
 * so the link stays disabled and says why beside it; a lookup that failed says so and asks again
 * on a press. `look` is a quiet heading button, or a plain link inside a sentence.
 */
export function ChangesSinceSettled(props: {
  settled: string;
  compare?: string;
  look: "button" | "inline";
}): JSX.Element {
  const { settled, compare = settled, look } = props;
  const { lookup, retry } = useSettledVersion(settled);
  const [open, setOpen] = useState(false);
  const link = useRef<HTMLAnchorElement>(null);
  const reasonId = useId();
  const anchorProps =
    look === "button"
      ? { ...HEADING_BUTTON, className: `${HEADING_BUTTON.className} trace-changes` }
      : { className: "trace-changes" };
  const explained = lookup.kind === "uncommitted" || lookup.kind === "failed";
  const disabled = lookup.kind === "asking" || lookup.kind === "uncommitted";

  return (
    <>
      <a
        ref={link}
        href={sourceHref(compare)}
        {...anchorProps}
        aria-disabled={disabled || undefined}
        aria-busy={lookup.kind === "asking" || undefined}
        aria-describedby={explained ? reasonId : undefined}
        onClick={(event) => {
          event.preventDefault();
          if (lookup.kind === "commit") setOpen(true);
          else if (lookup.kind === "failed") retry();
        }}
      >
        {look === "button" && <FileDiff aria-hidden="true" />}
        See what changed
      </a>
      <LookupReason lookup={lookup} id={reasonId} look={look} />
      {open && lookup.kind === "commit" && (
        <SettledDialog
          path={compare}
          commit={lookup.commit}
          link={link}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
