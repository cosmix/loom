import type { ReactNode } from "react";

import type { Severity, TraceFinding } from "@/plan/api/schema-trace";

import { stateWord, type CardState } from "./findings-model";

// The two marks a finding wears on the board and in its dialog: its state (a glyph and its word)
// and its severity (three bars and its word). Only the glyph takes a tone (DESIGN.md, Status), so
// the words keep the text's ink; a state is never its hue alone.

type Tone = "warning" | "completed" | "blocked";

interface Glyph {
  tone: Tone | null;
  paths: ReactNode;
}

const RING = <circle cx="12" cy="12" r="9" />;

const GLYPHS: Record<CardState, Glyph> = {
  open: {
    tone: "warning",
    paths: (
      <>
        {RING}
        <circle cx="12" cy="12" r="3.5" fill="currentColor" />
      </>
    ),
  },
  addressing: {
    tone: "warning",
    paths: (
      <>
        {RING}
        <circle cx="12" cy="12" r="6" fill="currentColor" />
      </>
    ),
  },
  unverified: {
    tone: null,
    paths: (
      <>
        {RING}
        <path d="M9.3 9.2a2.8 2.8 0 0 1 5.4 1c0 1.9-2.7 2.3-2.7 3.8M12 17h.01" />
      </>
    ),
  },
  stale: { tone: null, paths: <circle cx="12" cy="12" r="9" strokeDasharray="3.2 3.9" /> },
  duplicate: {
    tone: null,
    paths: (
      <>
        <rect x="8" y="8" width="13" height="13" rx="2" />
        <path d="M5 16a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2" />
      </>
    ),
  },
  addressed: {
    tone: "completed",
    paths: (
      <>
        {RING}
        <path d="m8.5 12.5 2.5 2.5 4.5-5" />
      </>
    ),
  },
  unaddressed: {
    tone: "blocked",
    paths: (
      <>
        {RING}
        <path d="m5.7 5.7 12.6 12.6" />
      </>
    ),
  },
  dismissed: { tone: null, paths: <path d="M18 6 6 18M6 6l12 12" /> },
};

/** The state's glyph and its word; a duplicate's link to the finding it repeats is the caller's. */
export function StateMark(props: { entry: TraceFinding; state: CardState; children?: ReactNode }) {
  const { entry, state, children } = props;
  const glyph = GLYPHS[state];
  return (
    <span className="findings-state" data-tone={glyph.tone ?? undefined}>
      <svg
        className="findings-glyph"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        {glyph.paths}
      </svg>
      <span>{children ?? stateWord(entry, state)}</span>
    </span>
  );
}

const SEVERITY_WORD: Record<Severity, string> = { high: "High", medium: "Medium", low: "Low" };

/** Three bars filled to the severity, and its word; the ledger may carry none. */
export function SeverityMark({ severity }: { severity: Severity | null }) {
  return (
    <span className="findings-severity" data-severity={severity ?? "none"}>
      <svg className="findings-glyph" viewBox="0 0 16 16" aria-hidden="true">
        <rect x="1" y="10" width="3.6" height="5" rx="1" />
        <rect data-bar="2" x="6.2" y="6" width="3.6" height="9" rx="1" />
        <rect data-bar="3" x="11.4" y="2" width="3.6" height="13" rx="1" />
      </svg>
      <span>{severity === null ? "No severity" : SEVERITY_WORD[severity]}</span>
      {severity !== null && (
        <>
          {" "}
          <span className="sr-only">severity</span>
        </>
      )}
    </span>
  );
}
