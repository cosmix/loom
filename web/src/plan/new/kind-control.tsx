import { useId } from "react";

import "./kind-control.css";

const CHOICES = [
  { small: false, label: "Feature", hint: "A vision, product sections, a metaplan and plans" },
  { small: true, label: "Standalone plan", hint: "One plan in doc/plans/" },
] as const;

/**
 * The first decision on New work: a feature or a standalone plan. Two radios, each named by its
 * label alone, with the description attached through `aria-describedby`.
 */
export function KindControl({
  small,
  onChange,
  disabled,
}: {
  small: boolean;
  onChange: (small: boolean) => void;
  disabled: boolean;
}) {
  const group = useId();
  return (
    <fieldset className="kickoff-kind" disabled={disabled}>
      <legend className="kickoff-kind__legend">Kind of work</legend>
      {CHOICES.map((choice) => {
        const id = `${group}-${choice.small ? "plan" : "feature"}`;
        return (
          <div key={choice.label} className="kickoff-kind__choice">
            <input
              id={id}
              type="radio"
              name={group}
              checked={small === choice.small}
              aria-describedby={`${id}-hint`}
              onChange={() => onChange(choice.small)}
            />
            <label htmlFor={id} className="kickoff-kind__label">
              {choice.label}
            </label>
            <span id={`${id}-hint`} className="kickoff-kind__hint">
              {choice.hint}
            </span>
          </div>
        );
      })}
    </fieldset>
  );
}
