import { Fragment, useEffect, useId, useRef, useState } from "react";

import type { TraceEntry } from "@/plan/api/schema-trace";
import { IsolatedSentence } from "@/plan/concept/sentence-text";
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
          <IsolatedSentence text={candidate.title} />
          {candidate === choices.suggested && (
            <span className="plan-fine maps-picker__suggested">Suggested</span>
          )}
        </label>
      ))}
      {choices.leftOut.length > 0 && (
        <p className="plan-fine maps-picker__note">
          Not offered, because their plan is running or done:{" "}
          {choices.leftOut.map((left, index) => (
            <Fragment key={left.anchor}>
              {index > 0 && "; "}
              <IsolatedSentence text={left.title} />
            </Fragment>
          ))}
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
            Fold into <IsolatedSentence text={entry.title} />
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
 * names the chosen entry. It takes focus as it opens, on the chosen radio (else the first, else
 * Cancel), so the keyboard is in the group the opener just drew; `id` is what the opener controls.
 */
export function FoldPicker(props: {
  id: string;
  choices: FoldChoices;
  onPick: (entry: TraceEntry) => void;
  onCancel: () => void;
}) {
  const { id, choices, onPick, onCancel } = props;
  const name = useId();
  const root = useRef<HTMLFieldSetElement>(null);
  const [chosen, setChosen] = useState<string | null>(choices.suggested?.anchor ?? null);
  const entry = choices.entries.find((candidate) => candidate.anchor === chosen);
  useEffect(() => {
    const picker = root.current;
    const first =
      picker?.querySelector<HTMLElement>("input:checked") ??
      picker?.querySelector<HTMLElement>("input") ??
      [...(picker?.querySelectorAll("button") ?? [])].at(-1);
    first?.focus();
  }, []);
  return (
    <fieldset id={id} ref={root} className="maps-picker">
      <legend>Fold into which entry</legend>
      <FoldOptions choices={choices} name={name} chosen={chosen} onChoose={setChosen} />
      <FoldActions entry={entry} onPick={onPick} onCancel={onCancel} />
    </fieldset>
  );
}
