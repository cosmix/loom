import { useAtomValue } from "jotai";
import { History } from "lucide-react";
import { useState, type ReactNode } from "react";
import { useInRouterContext, useLocation, useNavigate, type Location } from "react-router";

import { Button } from "@/components/ui/button";
import { useMediaQuery } from "@/lib/use-media-query";
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
import { seenBaseline, seenTextsAtom } from "./seen-text";

export { useChangedSince } from "./changed-since-data";

/** The route of the "Changed since you looked" page. */
export const CHANGES_PATH = "/changes";

/** From 1024px the list and the diff sit side by side; under it the diff opens under its row. */
const SIDE_BY_SIDE = "(min-width: 1024px)";

/**
 * Where the page's way back leads: the in-app path the trigger stored in the location state, or
 * null. Only a same-origin path is taken, so a crafted history entry cannot point the link away.
 */
export function changesFrom(state: unknown): string | null {
  const from = (state as { from?: unknown } | null)?.from;
  // Browsers read "/\" as "//", so a leading backslash after the slash is rejected too.
  const external = typeof from === "string" && (from.startsWith("//") || from.startsWith("/\\"));
  return typeof from === "string" && from.startsWith("/") && !external ? from : null;
}

/** The state the trigger navigates with: the page it was opened from, kept across reopening. */
function returnState(location: Location): { from: string | null } {
  if (location.pathname === CHANGES_PATH) return { from: changesFrom(location.state) };
  return { from: `${location.pathname}${location.search}${location.hash}` };
}

function caption(baseline: Baseline, ref: Comparison | null) {
  if (ref)
    return (
      <>
        Compared with <Untrusted text={ref.name} />
      </>
    );
  return baseline.kind === "seen" ? (
    "Since you viewed it"
  ) : (
    <>
      Since commit <Untrusted text={shortSha(baseline.commit)} />
    </>
  );
}

/** `underRow`: drawn as the item under its row, which already names the path. */
function SelectedDiff({ selection, underRow }: { selection: Selection; underRow: boolean }) {
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
        {!underRow && <PathText path={selection.path} />}
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
    // `aria-disabled`, not `disabled`: the button keeps its focus while it loads, and a disabled
    // button drops it to the page body. `markAll` ignores presses while pending.
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
  /** The listed documents in checkpoint mode; null while a ref is compared or none is left. */
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

function ComparisonBody({
  rows,
  selection,
  since,
  sourceOf,
  status,
  onOpen,
}: {
  rows: Row[];
  selection: Selection | null;
  since: ReactNode;
  sourceOf: (path: string) => RowSource;
  status: ReactNode;
  onOpen: (row: Row) => void;
}) {
  const sideBySide = useMediaQuery(SIDE_BY_SIDE);
  const diff = selection && (
    <SelectedDiff
      key={`${selection.ref?.commit ?? ""}:${selection.path}`}
      selection={selection}
      underRow={!sideBySide}
    />
  );
  return (
    <div className="changed-since__body">
      <div className="changed-since__index">
        {status}
        <ChangeRows
          rows={rows}
          selected={selection?.path ?? null}
          sourceOf={sourceOf}
          since={since}
          detail={sideBySide ? null : diff}
          onOpen={onOpen}
        />
      </div>
      {sideBySide &&
        (diff || (
          <p className="plan-empty changed-since__placeholder">
            Choose a document to see what changed in it.
          </p>
        ))}
    </div>
  );
}

/**
 * The list of changed documents, the open document's diff and the page foot. `empty` stands in
 * for the list once nothing is left to review in checkpoint mode (at arrival, or after Mark all
 * seen); the foot stays, so a ref can still be compared.
 */
export function ComparisonList({ changes, empty }: { changes: Change[]; empty: ReactNode }) {
  const mark = useMarkAllSeen();
  const [ref, setRef] = useState<Comparison | null>(null);
  const [selection, setSelection] = useState<Selection | null>(null);
  const { commit, compare, unavailable, rows, sourceOf } = useComparisonRows(
    changes,
    ref,
    selection,
  );
  const nothing = !ref && rows.length === 0;
  return (
    <>
      {nothing ? (
        empty
      ) : (
        <ComparisonBody
          rows={rows}
          selection={selection}
          sourceOf={sourceOf}
          status={<CompareStatus compare={compare} comparison={ref} unavailable={unavailable} />}
          since={
            commit && (
              <>
                Changes since commit <Untrusted text={shortSha(commit)} />
              </>
            )
          }
          onOpen={(row) => {
            // The open document refetches as it changes, so opening it again changes nothing.
            if (row.path !== selection?.path) setSelection({ path: row.path, ref });
          }}
        />
      )}
      <ComparisonFoot
        mark={mark}
        paths={ref || nothing ? null : rows.map((row) => row.path)}
        onRef={(value) => {
          setRef(value);
          setSelection(null);
        }}
      />
    </>
  );
}

/** The footer row of the activity popover: a tile, the title and the count, in one button. */
function RowTrigger({ count, onClick }: { count: number; onClick: () => void }) {
  return (
    <button
      type="button"
      className="activity-row"
      aria-label={`Changed since you looked · ${count}`}
      onClick={onClick}
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

/**
 * The activity popover's footer row, which opens the "Changed since you looked" page. The page
 * focuses its heading on arrival; that focus lands outside the popover, which dismisses it, and a
 * non-modal Radix popover dismissed from outside does not return focus to its trigger.
 */
export function ChangedSinceControl() {
  // The row navigates, so it needs a router. A pill mounted without one (the popover's own tests)
  // has no page to open and draws no row.
  return useInRouterContext() ? <RoutedChangedSinceControl /> : null;
}

function RoutedChangedSinceControl() {
  const changes = useChangedSince();
  const projectKey = useAtomValue(serverInfoAtom)?.project_key;
  const navigate = useNavigate();
  const location = useLocation();
  if (!projectKey || !changes.length) return null;
  return (
    <RowTrigger
      count={changes.length}
      onClick={() => void navigate(CHANGES_PATH, { state: returnState(location) })}
    />
  );
}
