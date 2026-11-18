import type { JSX } from "react";

import type { NodeRef, TraceFinding, TraceView } from "@/plan/api/schema-trace";
import { Board } from "@/plan/trace/findings-cards";
import { boardLanes } from "@/plan/trace/findings-model";
import { ignoreAction } from "@/plan/help/sample";

// The Findings board as the page draws it (`trace/findings-cards.tsx`), from a ledger written here:
// a help scene never requests live data. One finding sits in each lane: an unverified one, an open
// one that waits for the reader, an addressed one with what it changed, a dismissed one.

const FEATURE = "atlas";
const STOPS: NodeRef = { path: "doc/specs/stops.md", anchor: "stops" };
const OWNERS: NodeRef = { path: "doc/specs/routes.md", anchor: "ownership" };
const NAMES: NodeRef = { path: "doc/specs/routes.md", anchor: "flag-names" };

// Only what a card reads: the headings that name where it is about.
const TRACE: TraceView = {
  documents: [],
  headings: [
    { path: STOPS.path, anchor: "stops", title: "Stops", covered_by: [], findings: [] },
    { path: OWNERS.path, anchor: "ownership", title: "Ownership", covered_by: [], findings: [] },
    { path: NAMES.path, anchor: "flag-names", title: "Flag names", covered_by: [], findings: [] },
  ],
  entries: [],
  findings: [],
  gaps: [],
  frontier: [],
  problems: [],
};

function entry(
  id: string,
  title: string,
  about: NodeRef,
  parts: Pick<TraceFinding, "check" | "changed"> & Pick<TraceFinding["finding"], "status">,
): TraceFinding {
  return {
    finding: {
      id,
      title,
      about: [`${about.path}#${about.anchor}`],
      evidence: null,
      severity: "high",
      source: null,
      status: parts.status,
      text: "",
    },
    about: [about],
    check: parts.check,
    changed: parts.changed,
  };
}

const VERIFIED = { kind: "verified", duplicate_of: null } as const;

const FINDINGS: TraceFinding[] = [
  entry("F-5", "Flag names differ between the spec and the plan", NAMES, {
    check: { kind: "unverified", duplicate_of: null },
    changed: [],
    status: { kind: "open", detail: null },
  }),
  entry("F-3", "Stops have no time window", STOPS, {
    check: VERIFIED,
    changed: [],
    status: { kind: "open", detail: null },
  }),
  entry("F-4", "Routes never say who owns a stop", OWNERS, {
    check: VERIFIED,
    changed: [OWNERS],
    status: { kind: "addressed", detail: null },
  }),
  entry("F-2", "Rename the flag", NAMES, {
    check: VERIFIED,
    changed: [],
    status: { kind: "dismissed", detail: "code-level note" },
  }),
];

const LANES = boardLanes(FINDINGS, false);

export function FindingSpecimen(): JSX.Element {
  return (
    <div data-loom-plan inert>
      <Board
        trace={TRACE}
        feature={FEATURE}
        ledger={{ hash: "sample", problem: null }}
        addressing={false}
        lanes={LANES}
        onOpen={ignoreAction}
        actions={{}}
      />
    </div>
  );
}
