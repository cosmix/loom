import type { JSX } from "react";

import "./segmented-toggle.css";

export interface SegmentedToggleProps<T extends string> {
  /** The group's accessible name. */
  label: string;
  options: readonly { value: T; label: string }[];
  value: T;
  onChange(value: T): void;
}

/**
 * Two or more flush segments, one pressed: a choice between views of the same thing. Each is a
 * button with `aria-pressed`, named by its label. The label is also set as `data-label`, which the
 * stylesheet draws unseen at the pressed weight to hold the segment's width.
 */
export function SegmentedToggle<T extends string>(props: SegmentedToggleProps<T>): JSX.Element {
  return (
    <div role="group" aria-label={props.label} className="segmented-toggle">
      {props.options.map(({ value, label }) => (
        <button
          key={value}
          type="button"
          aria-pressed={props.value === value}
          data-label={label}
          onClick={() => props.onChange(value)}
        >
          {label}
        </button>
      ))}
    </div>
  );
}
