import { useEffect, useRef, useState, type FormEvent } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { openDiff } from "@/plan/diff/open-diff";
import { LoadState } from "@/plan/ui/load-state";

import { resolveRef } from "./api";
import type { changedSince } from "./checkpoint";

export type Change = ReturnType<typeof changedSince>[number];
export type Comparison = { commit: string; name: string };

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

export function RefForm({ onCompare }: { onCompare: (value: Comparison | null) => void }) {
  const { name, setName, error, busy, submit, reset } = useResolveRef(onCompare);
  return (
    <div className="space-y-2">
      <form onSubmit={(event) => void submit(event)} className="flex flex-wrap items-center gap-2">
        <label htmlFor="changed-since-ref">Compare with ref</label>
        <input
          id="changed-since-ref"
          className="min-w-0 rounded border px-2 py-1 font-mono"
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder="main or a tag"
        />
        <Button type="submit" size="sm" disabled={busy || !name.trim()}>
          Compare
        </Button>
        <Button type="button" size="sm" variant="ghost" onClick={reset}>
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
    </div>
  );
}

export function Limits() {
  return (
    <div className="space-y-1 text-[12px] text-muted-foreground">
      <p>
        Uncommitted edits at the checkpoint are compared with committed text; a diff can include
        edits you already saw.
      </p>
      <p>
        An unavailable checkpoint commit cannot supply old text. Use an uncommitted diff or compare
        with a ref.
      </p>
      <p>
        Local storage belongs to this page’s origin, including its port, and to this browser.
        Another browser or cleared storage starts fresh.
      </p>
    </div>
  );
}

export function ChangeRows({
  rows,
  available,
  checkpointMode,
  onOpen,
}: {
  rows: Change[];
  available: boolean;
  checkpointMode: boolean;
  onOpen: (path: string) => void;
}) {
  if (!rows.length) return <LoadState state="empty" title="No changed documents" />;
  return (
    <ul aria-label="Changed documents" className="divide-y divide-hairline">
      {rows.map((row) => (
        <li key={row.path} className="space-y-1 py-2">
          <div className="flex flex-wrap items-baseline gap-2">
            <span className="break-all font-mono">
              <Untrusted text={row.path} />
            </span>
            <span>{row.change}</span>
            {available && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => onOpen(row.path)}
                aria-label={`Show diff for ${visibleControls(row.path)}`}
              >
                Show diff
              </Button>
            )}
            <Link className="underline" to={openDiff("head", row.path)}>
              Uncommitted diff
            </Link>
          </div>
          {checkpointMode && (
            <p className="text-[12px] text-muted-foreground">
              If this document had uncommitted edits at the checkpoint, its diff uses committed text
              and can include edits you already saw.
            </p>
          )}
        </li>
      ))}
    </ul>
  );
}
