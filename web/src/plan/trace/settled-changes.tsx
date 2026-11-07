import { FileDiff } from "lucide-react";
import { useMemo, useRef, useState, type JSX, type RefObject } from "react";

import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { apiGet } from "@/plan/api/client";
import { settledVersionSchema } from "@/plan/api/schema-trace";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { DiffView } from "@/plan/diff/diff-view";
import { docRel } from "@/plan/files/address";
import { sourceHref } from "@/plan/feature/source-href";
import { HEADING_BUTTON } from "@/plan/ui/doc-heading";
import { useSelectedDiff } from "@/plan/review/changed-since-data";
import { shortSha } from "@/plan/review/changed-since-parts";
import { returnFocusTo } from "@/plan/review/focus-return";
import { LoadState } from "@/plan/ui/load-state";

/** What asking for the settled version came to: nothing yet, a commit to compare, or why not. */
type Lookup =
  | { kind: "closed" }
  | { kind: "asking" }
  | { kind: "commit"; commit: string }
  | { kind: "uncommitted" }
  | { kind: "failed"; message: string };

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
 * "See what changed": a link that, on click, asks which commit holds `settled`'s current Settled
 * line and opens the commit comparison of `compare` (default: the same document) against it. A
 * line that was never committed has no version to compare with, and the control says so. `look`
 * is a quiet heading button, or a plain link inside a sentence.
 */
export function ChangesSinceSettled(props: {
  settled: string;
  compare?: string;
  look: "button" | "inline";
}): JSX.Element {
  const { settled, compare = settled, look } = props;
  const [lookup, setLookup] = useState<Lookup>({ kind: "closed" });
  const link = useRef<HTMLAnchorElement>(null);
  const anchorProps =
    look === "button"
      ? { ...HEADING_BUTTON, className: `${HEADING_BUTTON.className} trace-changes` }
      : { className: "trace-changes" };

  const open = async () => {
    setLookup({ kind: "asking" });
    const result = await apiGet(
      `/api/settled-version?path=${encodeURIComponent(docRel(settled))}`,
      settledVersionSchema,
    );
    if (!result.ok) setLookup({ kind: "failed", message: result.message });
    else if (result.value.commit === null) setLookup({ kind: "uncommitted" });
    else setLookup({ kind: "commit", commit: result.value.commit });
  };

  return (
    <>
      <a
        ref={link}
        href={sourceHref(compare)}
        {...anchorProps}
        onClick={(event) => {
          event.preventDefault();
          void open();
        }}
      >
        {look === "button" && <FileDiff aria-hidden="true" />}
        See what changed
      </a>
      {lookup.kind === "uncommitted" && (
        <span role="status" className="trace-settle-note">
          The settled version is not committed
        </span>
      )}
      {lookup.kind === "failed" && (
        <span role="alert" className="trace-settle-note">
          <Untrusted text={lookup.message} />
        </span>
      )}
      {lookup.kind === "commit" && (
        <SettledDialog
          path={compare}
          commit={lookup.commit}
          link={link}
          onClose={() => setLookup({ kind: "closed" })}
        />
      )}
    </>
  );
}
