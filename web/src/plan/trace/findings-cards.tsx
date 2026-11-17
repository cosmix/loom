import { Check } from "lucide-react";
import { useId, type ReactNode } from "react";

import type { TraceFinding } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";

import type { Context } from "./findings-controls";
import { SeverityMark, StateMark } from "./findings-marks";
import {
  cardState,
  findingSource,
  nodeLabel,
  stateReason,
  type Lane,
  type LaneKey,
} from "./findings-model";

import "./findings-board.css";

// The Findings board: four lanes in the order a finding moves (verified, decided by the reader,
// fixed by a team; the fourth is off that path). A lane says what its state means and owns the
// action for it; a card is the finding's title, severity and place, and a press opens the
// finding in its dialog (`findings-sheet.tsx`). Every ledger string is drawn as text (`Untrusted`).

interface LaneText {
  name: string;
  step: string;
  note: string;
  none: string;
}

export const LANE_TEXT: Record<LaneKey, LaneText> = {
  verify: {
    name: "To verify",
    step: "Step 1",
    note: "No quote checked yet, or the quoted text has changed. A review of the feature verifies them.",
    none: "Every finding has a checked quote.",
  },
  decide: {
    name: "Yours to decide",
    step: "Step 2",
    note: "Verified and still true. Leave one and the next address run fixes it; dismiss it if it is wrong.",
    none: "Nothing waits for you.",
  },
  addressed: {
    name: "Addressed",
    step: "Step 3",
    note: "Fixed in the documents by a team. Each card names what changed.",
    none: "Nothing addressed yet.",
  },
  aside: {
    name: "Set aside",
    step: "Off the path",
    note: "Dismissed by you with a reason, or a repeat of another finding.",
    none: "Nothing set aside.",
  },
};

/** One finding on the board: a press opens it. The `li` carries the address `#finding-<id>`. */
function Card(props: Context & { entry: TraceFinding; onOpen: (id: string) => void }) {
  const { entry, trace, feature, addressing, onOpen } = props;
  const { finding } = entry;
  const state = cardState(entry, addressing);
  const reason = stateReason(entry, state);
  const first = entry.about[0];
  const spare = entry.about.length - 1;
  const source = findingSource(finding.source, feature);
  return (
    <li id={`finding-${finding.id}`} className="findings-card" data-state={state}>
      <button type="button" aria-haspopup="dialog" onClick={() => onOpen(finding.id)}>
        <span className="findings-card__top">
          <span className="findings-id">
            <Untrusted text={finding.id} />
          </span>
          <SeverityMark severity={finding.severity} />
        </span>
        <span className="findings-card__title">
          <Untrusted text={finding.title} />
        </span>
        {first && (
          <span className="findings-card__where">
            <Untrusted text={nodeLabel(trace, first, feature)} />
            {spare > 0 && <span className="findings-card__spare"> +{spare}</span>}
          </span>
        )}
        {state !== "open" && (
          <span className="findings-card__reason">
            <StateMark entry={entry} state={state}>
              {state === "addressed" ? reason : undefined}
            </StateMark>
            {reason !== null && state !== "addressed" && (
              <span className="findings-card__detail">
                <Untrusted text={reason} />
              </span>
            )}
          </span>
        )}
        <span className="findings-card__source">{source.short}</span>
      </button>
    </li>
  );
}

function LaneColumn(
  props: Context & { lane: Lane; onOpen: (id: string) => void; action: ReactNode },
) {
  const { lane, action, onOpen, ...rest } = props;
  const text = LANE_TEXT[lane.key];
  const heading = useId();
  return (
    <section
      className="findings-lane"
      data-lane={lane.key}
      data-hot={lane.key === "decide" && lane.entries.length > 0 ? "true" : undefined}
      aria-labelledby={heading}
    >
      <header className="findings-lane__head">
        <span className="eyebrow">{text.step}</span>
        <h2 id={heading}>
          {text.name} <span className="findings-lane__count">{lane.entries.length}</span>
        </h2>
        <p>{text.note}</p>
        <div className="findings-lane__action">{action}</div>
      </header>
      {lane.entries.length > 0 ? (
        <ul className="findings-lane__cards">
          {lane.entries.map((entry) => (
            <Card key={entry.finding.id} entry={entry} onOpen={onOpen} {...rest} />
          ))}
        </ul>
      ) : (
        <p className="findings-lane__none">
          <Check aria-hidden="true" />
          {text.none}
        </p>
      )}
    </section>
  );
}

/**
 * The four lanes. `actions` are the lane-owned controls (Review the feature under To verify,
 * Address the findings under Yours to decide), drawn by the page that owns those offers.
 */
export function Board(
  props: Context & {
    lanes: Lane[];
    onOpen: (id: string) => void;
    actions: Partial<Record<LaneKey, ReactNode>>;
  },
) {
  const { lanes, actions, ...rest } = props;
  return (
    <div className="findings-board">
      <div className="findings-board__lanes">
        {lanes.map((lane) => (
          <LaneColumn key={lane.key} lane={lane} action={actions[lane.key]} {...rest} />
        ))}
      </div>
    </div>
  );
}
