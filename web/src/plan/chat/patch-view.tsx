import type { JSX } from "react";

import { Untrusted } from "./visible-controls";

function lineTone(line: string, inHunk: boolean): string | undefined {
  if (!inHunk && (line.startsWith("+++") || line.startsWith("---"))) return undefined;
  if (line.startsWith("+")) return "var(--tone-completed)";
  if (line.startsWith("-")) return "var(--tone-blocked)";
  return undefined;
}

export function PatchView({ patch }: { patch: string }): JSX.Element {
  const lines = patch.split("\n");
  const firstHunk = lines.findIndex((line) => line.startsWith("@@"));
  return (
    <pre className="min-w-0 whitespace-pre-wrap break-words font-mono text-xs leading-relaxed">
      {lines.map((line, index) => (
        <span key={index} style={{ color: lineTone(line, firstHunk >= 0 && index > firstHunk) }}>
          <Untrusted text={line} />
          {index < lines.length - 1 ? "\n" : null}
        </span>
      ))}
    </pre>
  );
}
