import { Check, Minus, PenLine, Plus, type LucideIcon } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { openDiff } from "@/plan/diff/open-diff";
import { LoadState } from "@/plan/ui/load-state";

import { resolveRef } from "./api";
import type { changedSince } from "./checkpoint";

export type Change = ReturnType<typeof changedSince>[number];
export type Comparison = { commit: string; name: string };
/** A listed row: a change, or "seen" for the open document once its diff marked it seen. */
export type Row = { path: string; change: Change["change"] | "seen" };
/** What a row's diff starts from; null when it has none (only its uncommitted diff). */
export type RowSource = "seen" | "commit" | "ref" | null;

export const shortSha = (commit: string) => commit.slice(0, 7);

function useResolveRef(onCompare: (value: Comparison | null) => void) {
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const version = useRef(0);
  useEffect(
    () => () => {
      version.current++;
    },
    [],
  );
  async function submit(event: FormEvent) {
    event.preventDefault();
    const started = ++version.current;
    setBusy(true);
    setError(null);
    const result = await resolveRef(name.trim());
    if (started !== version.current) return;
    setBusy(false);
    if (result.ok) onCompare({ commit: result.value.commit, name: name.trim() });
    else setError(result.message);
  }
  function reset() {
    version.current++;
    setBusy(false);
    setError(null);
    onCompare(null);
  }
  return { name, setName, error, busy, submit, reset };
}

/** "Compare with a ref…": a quiet disclosure in the dialog's foot holding the ref form. */
export function RefForm({ onCompare }: { onCompare: (value: Comparison | null) => void }) {
  const { name, setName, error, busy, submit, reset } = useResolveRef(onCompare);
  return (
    <details className="changed-since__ref">
      <summary className="changed-since__link">Compare with a ref…</summary>
      <form onSubmit={(event) => void submit(event)} className="changed-since__ref-form">
        <label htmlFor="changed-since-ref" className="plan-fine">
          Compare with ref
        </label>
        <input
          id="changed-since-ref"
          className="changed-since__input"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="main or a tag"
        />
        <Button
          type="submit"
          size="sm"
          className="plan-button"
          data-look="secondary"
          disabled={busy || !name.trim()}
        >
          Compare
        </Button>
        <Button type="button" size="sm" className="plan-button" data-look="quiet" onClick={reset}>
          Use checkpoint
        </Button>
      </form>
      {error && (
        <LoadState
          state="error"
          title="The ref could not be resolved"
          message={visibleControls(error)}
        />
      )}
    </details>
  );
}

/** The foot's note: where the record lives, and what a diff without a seen copy can include. */
export function Limits() {
  return (
    <p className="changed-since__note plan-fine">
      What you saw is kept in this browser, per origin and port. Without it, a diff starts at the
      checkpoint commit and can repeat uncommitted edits.
    </p>
  );
}

const STATUS: Record<Row["change"], { icon: LucideIcon; tone: string }> = {
  added: { icon: Plus, tone: "tone-completed" },
  changed: { icon: PenLine, tone: "tone-executing" },
  removed: { icon: Minus, tone: "tone-blocked" },
  seen: { icon: Check, tone: "tone-dimmed" },
};

function RowStatus({ change }: { change: Row["change"] }) {
  const { icon: Icon, tone } = STATUS[change];
  return (
    <span className="plan-status changed-since__status">
      <Icon className={tone} aria-hidden="true" />
      {change}
    </span>
  );
}

/** A path in mono on one line, truncated from its start so the file name stays in view. */
export function PathText({ path }: { path: string }) {
  return (
    <span className="changed-since__path" title={visibleControls(path)}>
      <bdi>
        <Untrusted text={path} />
      </bdi>
    </span>
  );
}

function RowAction({ row, source, onOpen }: RowProps) {
  if (source === null) {
    return (
      <Link className="changed-since__link" to={openDiff("head", row.path)}>
        Uncommitted diff
      </Link>
    );
  }
  return (
    <Button
      size="sm"
      className="plan-button"
      data-look="quiet"
      onClick={() => onOpen(row)}
      aria-label={`Show diff for ${visibleControls(row.path)}`}
    >
      View changes
    </Button>
  );
}

type RowProps = { row: Row; source: RowSource; onOpen: (row: Row) => void };

export function ChangeRows({
  rows,
  selected,
  sourceOf,
  since,
  onOpen,
}: {
  rows: Row[];
  selected: string | null;
  sourceOf: (path: string) => RowSource;
  /** The fine line under a row whose diff starts from the checkpoint commit. */
  since: ReactNode;
  onOpen: (row: Row) => void;
}) {
  if (!rows.length) return <LoadState state="empty" title="No changed documents" />;
  return (
    <ul aria-label="Changed documents" className="changed-since__list">
      {rows.map((row) => {
        const source = sourceOf(row.path);
        return (
          <li
            key={row.path}
            className="changed-since__row"
            aria-current={row.path === selected || undefined}
          >
            <PathText path={row.path} />
            <span className="changed-since__meta">
              <RowStatus change={row.change} />
              {source === "commit" && row.change !== "seen" && (
                <span className="plan-fine">{since}</span>
              )}
            </span>
            <RowAction row={row} source={source} onOpen={onOpen} />
          </li>
        );
      })}
    </ul>
  );
}
