import { useId, type ReactNode, type RefObject } from "react";

import { KIND_WORD } from "@/plan/new/kickoff-head";
import type { WorkKind } from "@/plan/new/kickoff-start";

import "./kind-control.css";

const CHOICES: ReadonlyArray<{ kind: WorkKind; label: string; hint: string }> = [
  {
    kind: "feature",
    label: KIND_WORD.feature,
    hint: "A vision, product sections, a metaplan and plans",
  },
  { kind: "plan", label: KIND_WORD.plan, hint: "One plan in doc/plans/" },
  { kind: "follow-up", label: "Follow-up", hint: "New plans for a feature that is already built" },
];

/**
 * The first decision on New work: a feature, a standalone plan or a follow-up. Three native radios
 * in one pill track, each named by its label alone and described by its hint through
 * `aria-describedby`. The chosen kind's hint shows beside the track unless `beside` (the picked
 * feature's chip) stands there; the other hints stay in the DOM for their radios, unseen.
 * `children` float under the track: the feature picker, anchored to it.
 */
export function KindControl({
  kind,
  onChange,
  disabled,
  beside,
  children,
  ref,
}: {
  kind: WorkKind;
  onChange: (kind: WorkKind) => void;
  disabled: boolean;
  beside?: ReactNode;
  children?: ReactNode;
  ref?: RefObject<HTMLFieldSetElement | null>;
}) {
  const group = useId();
  const hintId = (choice: WorkKind) => `${group}-${choice}-hint`;
  return (
    <fieldset ref={ref} className="kickoff-kind" disabled={disabled}>
      <legend className="sr-only">Kind of work</legend>
      <div className="kickoff-kind__anchor">
        <div className="kickoff-kind__track">
          {CHOICES.map((choice) => (
            <label key={choice.kind} className="kickoff-kind__choice">
              <input
                type="radio"
                name={group}
                value={choice.kind}
                checked={kind === choice.kind}
                aria-describedby={hintId(choice.kind)}
                onChange={() => onChange(choice.kind)}
              />
              {choice.label}
            </label>
          ))}
        </div>
        {children}
      </div>
      <KindHints kind={kind} shown={beside === undefined} hintId={hintId} />
      {beside}
    </fieldset>
  );
}

/** Every kind's hint, each the id its radio describes itself by; only the chosen one is seen. */
function KindHints({
  kind,
  shown,
  hintId,
}: {
  kind: WorkKind;
  shown: boolean;
  hintId: (choice: WorkKind) => string;
}) {
  return CHOICES.map((choice) => (
    <span
      key={choice.kind}
      id={hintId(choice.kind)}
      className={shown && choice.kind === kind ? "kickoff-kind__hint" : "sr-only"}
    >
      {choice.hint}
    </span>
  ));
}
