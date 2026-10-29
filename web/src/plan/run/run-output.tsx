import { useAtomValue } from "jotai";
import { useLayoutEffect, useRef } from "react";

import type { RunJob } from "@/plan/api/schema-run";

import { runOutputAtom } from "./use-run";

export function RunOutput({ job }: { job: RunJob }) {
  const lines = useAtomValue(runOutputAtom)[job];
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
      aria-label={`${job} output`}
      tabIndex={0}
      className="max-h-64 overflow-auto whitespace-pre-wrap break-words rounded border p-3 font-mono text-xs"
      onScroll={(event) => {
        const node = event.currentTarget;
        follow.current = node.scrollHeight - node.clientHeight - node.scrollTop <= 2;
      }}
    >
      {lines.join("\n")}
    </pre>
  );
}
