import { useId, useState } from "react";

import type { TraceEntry } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import type { FoldChoices } from "./findings-model";

function FoldOptions(props: {
  choices: FoldChoices;
  name: string;
  chosen: string | null;
  onChoose: (anchor: string) => void;
}) {
  const { choices, name, chosen, onChoose } = props;
  return (
    <>
      {choices.entries.length === 0 && (
        <p className="plan-fine">No metaplan entry is waiting for its plan.</p>
      )}
      {choices.entries.map((candidate) => (
        <label key={candidate.anchor} className="maps-picker__option">
          <input
            type="radio"
            name={name}
            checked={chosen === candidate.anchor}
            onChange={() => onChoose(candidate.anchor)}
          />
          <span>
            <Untrusted text={candidate.title} />
          </span>
          {candidate === choices.suggested && (
            <span className="plan-fine maps-picker__suggested">Suggested</span>
          )}
        </label>
      ))}
      {choices.leftOut.length > 0 && (
        <p className="plan-fine maps-picker__note">
          Not offered, because their plan is running or done:{" "}
          <Untrusted text={choices.leftOut.map((left) => left.title).join("; ")} />
        </p>
      )}
    </>
  );
}

function FoldActions(props: {
  entry: TraceEntry | undefined;
  onPick: (entry: TraceEntry) => void;
  onCancel: () => void;
}) {
  const { entry, onPick, onCancel } = props;
  return (
    <div className="maps-picker__actions">
      <DisabledReason
        reason={entry ? null : "Pick an entry first"}
        look="secondary"
        onClick={() => entry && onPick(entry)}
      >
        {entry ? (
          <>
            Fold into <Untrusted text={entry.title} />
          </>
        ) : (
          "Fold into the chosen entry"
        )}
      </DisabledReason>
      <DisabledReason reason={null} look="quiet" onClick={onCancel}>
        Cancel
      </DisabledReason>
    </div>
  );
}

/**
 * The inline picker of Fold into entry: a radio row per metaplan entry whose plan has not started,
 * the suggested entry first and selected, a note naming the entries left out, and a confirm that
 * names the chosen entry.
 */
export function FoldPicker(props: {
  choices: FoldChoices;
  onPick: (entry: TraceEntry) => void;
  onCancel: () => void;
}) {
  const { choices, onPick, onCancel } = props;
  const name = useId();
  const [chosen, setChosen] = useState<string | null>(choices.suggested?.anchor ?? null);
  const entry = choices.entries.find((candidate) => candidate.anchor === chosen);
  return (
    <fieldset className="maps-picker">
      <legend>Fold into which entry</legend>
      <FoldOptions choices={choices} name={name} chosen={chosen} onChoose={setChosen} />
      <FoldActions entry={entry} onPick={onPick} onCancel={onCancel} />
    </fieldset>
  );
}
