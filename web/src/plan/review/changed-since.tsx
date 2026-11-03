import { useAtomValue } from "jotai";
import { History } from "lucide-react";
import { useRef, useState, type ComponentProps } from "react";

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
import { serverInfoAtom } from "@/plan/state/atoms";
import { ActivityTile } from "@/plan/ui/activity-glyphs";
import { LoadState } from "@/plan/ui/load-state";

import "./changed-since.css";
import {
  useChangedSince,
  useCompare,
  useMarkAllSeen,
  useSelectedDiff,
  type Baseline,
  type MarkAll,
  type Selection,
} from "./changed-since-data";
import {
  ChangeRows,
  Limits,
  PathText,
  RefForm,
  shortSha,
  type Change,
  type Comparison,
  type Row,
  type RowSource,
} from "./changed-since-parts";
import { checkpointAtom } from "./checkpoint";
import { focusReadingRegion } from "./focus-return";
import { seenBaseline, seenTextsAtom } from "./seen-text";

export { useChangedSince } from "./changed-since-data";

function caption(baseline: Baseline, ref: Comparison | null) {
  if (ref)
    return (
      <>
        Compared with <Untrusted text={ref.name} />
      </>
    );
  return baseline.kind === "seen"
    ? "Since you viewed it"
    : `Since commit ${shortSha(baseline.commit)}`;
}

function SelectedDiff({ selection }: { selection: Selection }) {
  const { load, retry } = useSelectedDiff(selection);
  let body;
  if (!load) body = <LoadState state="loading" title="Loading the diff" />;
  else if ("error" in load)
    body = (
      <LoadState
        state="error"
        message={visibleControls(load.error)}
        action={{ label: "Retry diff", onClick: retry }}
      />
    );
  else body = <DiffView path={selection.path} before={load.before} after={load.after} />;
  return (
    <section className="changed-since__diff" aria-label="Selected document's changes">
      <div className="changed-since__diff-head">
        <PathText path={selection.path} />
        {load && !("error" in load) && (
          <span className="plan-fine">{caption(load.baseline, selection.ref)}</span>
        )}
      </div>
      {body}
    </section>
  );
}

function MarkAllButton({ mark, paths }: { mark: MarkAll; paths: string[] }) {
  return (
    // `aria-disabled`, not `disabled`: the button holds focus inside the modal while it loads, and
    // a disabled button drops it to the page body. `markAll` ignores presses while pending.
    <Button
      className="plan-button aria-disabled:cursor-not-allowed aria-disabled:opacity-50"
      data-look="secondary"
      aria-disabled={mark.pending || undefined}
      onClick={() => mark.markAll(paths)}
    >
      {mark.pending ? "Marking all seen…" : "Mark all seen"}
    </Button>
  );
}

function CompareStatus({
  compare,
  comparison,
  unavailable,
}: {
  compare: ReturnType<typeof useCompare>;
  comparison: Comparison | null;
  unavailable: boolean;
}) {
  return (
    <>
      {comparison && (
        <p className="changed-since__state">
          Comparing with <Untrusted text={comparison.name} />. This ref is not stored.
        </p>
      )}
      {compare.loading && <LoadState state="loading" title="Loading comparison" />}
      {compare.error && (
        <LoadState
          state="error"
          message={visibleControls(compare.error)}
          action={{ label: "Retry comparison", onClick: compare.retry }}
        />
      )}
      {unavailable && (
        <p role="status" className="changed-since__state">
          The checkpoint commit is unavailable, so documents you have not viewed here offer only
          their uncommitted diff.
        </p>
      )}
    </>
  );
}

/** The open document stays listed, marked current, after its diff marked it seen. */
function withSelected(rows: Row[], selection: Selection | null): Row[] {
  if (!selection || selection.ref || rows.some((row) => row.path === selection.path)) return rows;
  const seen: Row = { path: selection.path, change: "seen" };
  return [...rows, seen].sort((a, b) => (a.path < b.path ? -1 : a.path > b.path ? 1 : 0));
}

/**
 * The rows the list shows and where each row's diff starts. In checkpoint mode a row with an
 * intact seen text has its own baseline, so it offers a diff even when the commit is unavailable;
 * ref mode keeps one rule for every row.
 */
function useComparisonRows(changes: Change[], ref: Comparison | null, selection: Selection | null) {
  const checkpoint = useAtomValue(checkpointAtom);
  const entries = useAtomValue(seenTextsAtom);
  const commit = ref ? ref.commit : (checkpoint?.commit ?? null);
  const compare = useCompare(commit);
  const commitReady = commit !== null && compare.value?.available === true && !compare.loading;
  const unavailable = commit === null || compare.value?.available === false;
  const listed: Row[] = ref ? (compare.value?.docs ?? []) : changes;
  const rows = withSelected(
    unavailable ? listed.map((row): Row => ({ ...row, change: "changed" })) : listed,
    selection,
  );
  const sourceOf = (path: string): RowSource => {
    if (ref) return commitReady ? "ref" : null;
    if (seenBaseline(entries, checkpoint, path) !== null) return "seen";
    return commitReady ? "commit" : null;
  };
  return { commit, compare, unavailable, rows, sourceOf };
}

function ComparisonFoot({
  mark,
  paths,
  onRef,
}: {
  mark: MarkAll;
  /** The listed documents in checkpoint mode; null while a ref is compared. */
  paths: string[] | null;
  onRef: (value: Comparison | null) => void;
}) {
  return (
    <footer className="changed-since__foot">
      {paths && <MarkAllButton mark={mark} paths={paths} />}
      <RefForm onCompare={onRef} />
      {mark.error && (
        <LoadState
          state="error"
          title="Could not mark all seen"
          message={visibleControls(mark.error)}
        />
      )}
      <Limits />
    </footer>
  );
}

function ComparisonList({ changes }: { changes: Change[] }) {
  const mark = useMarkAllSeen();
  const [ref, setRef] = useState<Comparison | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const { commit, compare, unavailable, rows, sourceOf } = useComparisonRows(
    changes,
    ref,
    selection,
  );
  return (
    <>
      <div className="changed-since__body">
        <div className="changed-since__index">
          <CompareStatus compare={compare} comparison={ref} unavailable={unavailable} />
          <ChangeRows
            rows={rows}
            selected={selection?.path ?? null}
            sourceOf={sourceOf}
            since={commit && `Changes since commit ${shortSha(commit)}`}
            onOpen={(row) => {
              // The open document refetches as it changes, so opening it again changes nothing.
              if (row.path !== selection?.path) setSelection({ path: row.path, ref });
            }}
          />
        </div>
        {selection ? (
          <SelectedDiff
            key={`${selection.ref?.commit ?? ""}:${selection.path}`}
            selection={selection}
          />
        ) : (
          <p className="plan-empty changed-since__placeholder">
            Choose a document to see what changed in it.
          </p>
        )}
      </div>
      <ComparisonFoot
        mark={mark}
        paths={ref ? null : rows.map((row) => row.path)}
        onRef={(value) => {
          setRef(value);
          setSelection(null);
        }}
      />
    </>
  );
}

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
        <span className="activity-row__sub">Documents edited since you last viewed them</span>
      </span>{" "}
      {/* The spaces keep the row's text readable ("… them 2"); grid and flex layout ignore them. */}
      <span className="activity-row__time">{count}</span>
    </button>
  );
}

function ChangedSinceWidget({ changes }: { changes: Change[] }) {
  const [open, setOpen] = useState(false);
  const opener = useRef<HTMLButtonElement>(null);
  if (!changes.length && !open) return null;
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      {changes.length > 0 && (
        <DialogTrigger asChild>
          <RowTrigger ref={opener} count={changes.length} />
        </DialogTrigger>
      )}
      <DialogContent
        className="changed-since motion-reduce:animate-none"
        onCloseAutoFocus={(event) => {
          // "Mark all seen" removes the opener; Radix would leave focus on the page body.
          if (opener.current?.isConnected) return;
          event.preventDefault();
          focusReadingRegion();
        }}
      >
        <header className="changed-since__head">
          <DialogTitle className="plan-section-title">Changed since you looked</DialogTitle>
          <DialogDescription className="changed-since__lede">
            Documents that changed since you last looked at them in this browser.
          </DialogDescription>
          <p className="plan-fine">
            Opening a document’s changes marks it seen. Mark all seen starts over from now.
          </p>
        </header>
        {open && <ComparisonList changes={changes} />}
      </DialogContent>
    </Dialog>
  );
}

/** The activity popover's footer row, which opens the comparison dialog. */
export function ChangedSinceControl() {
  const changes = useChangedSince();
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  return projectKey ? <ChangedSinceWidget key={projectKey} changes={changes} /> : null;
}
