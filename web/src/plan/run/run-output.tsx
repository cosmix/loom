import { useAtomValue } from "jotai";
import { memo, useLayoutEffect, useRef, useState } from "react";

import type { RunJob } from "@/plan/api/schema-run";
import { Untrusted } from "@/plan/chat/visible-controls";

import { runOutputAtom } from "./use-run";

// The line break belongs to its line, so appending never changes an element a screen reader has
// already announced (role="log" is a live region).
const RunLine = memo(function RunLine({ line }: { line: string }) {
  return <Untrusted text={`${line}\n`} />;
});

// appendRunOutput only appends and trims the front (the 5,000-line cap), so a new list is the
// old one minus some leading lines plus new ones; a list sharing nothing with the old one drops
// everything. Revisit if the atom ever carries an absolute line index: key on that instead.
function droppedCount(previous: readonly string[], next: readonly string[]): number {
  for (let dropped = 0; dropped < previous.length; dropped++) {
    const kept = previous.length - dropped;
    let same = kept <= next.length;
    for (let index = 0; same && index < kept; index++) {
      same = previous[dropped + index] === next[index];
    }
    if (same) return dropped;
  }
  return previous.length;
}

/** The count of lines that came before `lines[0]`, which makes `first + index` a stable key. */
function useFirstLineId(lines: readonly string[]): number {
  const [seen, setSeen] = useState({ lines, first: 0 });
  if (seen.lines === lines) return seen.first;
  const next = { lines, first: seen.first + droppedCount(seen.lines, lines) };
  setSeen(next);
  return next.first;
}

export function RunOutput({ job }: { job: RunJob }) {
  const lines = useAtomValue(runOutputAtom)[job];
  const first = useFirstLineId(lines);
  const viewport = useRef<HTMLPreElement>(null);
  const follow = useRef(true);

  useLayoutEffect(() => {
    if (follow.current && viewport.current) {
      viewport.current.scrollTop = viewport.current.scrollHeight;
    }
  }, [lines, job]);

  return (
    <pre
      ref={viewport}
      role="log"
      aria-label={`${job} output`}
      tabIndex={0}
      className="max-h-64 overflow-auto whitespace-pre-wrap [overflow-wrap:anywhere] rounded border p-3 font-mono text-xs"
      onScroll={(event) => {
        const node = event.currentTarget;
        follow.current = node.scrollHeight - node.clientHeight - node.scrollTop <= 2;
      }}
    >
      {lines.map((line, index) => (
        <RunLine key={first + index} line={line} />
      ))}
    </pre>
  );
}
