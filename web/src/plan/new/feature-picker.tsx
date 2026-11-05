import { Check } from "lucide-react";
import { useEffect, useId, useRef, useState, type KeyboardEvent, type RefObject } from "react";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import type { FollowUpFeature } from "@/plan/new/follow-up-features";

import "./feature-picker.css";

const EMPTY = "No feature has a metaplan yet.";

/** A 14px ring in `--primary` filled by `fraction`, over a `--border` track. */
function ProgressRing({ fraction }: { fraction: number }) {
  const filled = Math.min(1, Math.max(0, fraction));
  return (
    <svg viewBox="0 0 14 14" className="kickoff-ring" aria-hidden="true">
      <circle className="kickoff-ring__track" cx="7" cy="7" r="5.5" />
      <circle
        className="kickoff-ring__fill"
        cx="7"
        cy="7"
        r="5.5"
        pathLength="1"
        strokeDasharray={`${filled} 1`}
      />
    </svg>
  );
}

/** A built feature's check in `--tone-completed`; else its progress ring, done over total. */
function FeatureMark({ feature }: { feature: FollowUpFeature }) {
  return (
    <span className="kickoff-mark" data-built={feature.built || undefined} aria-hidden="true">
      {feature.built ? (
        <Check />
      ) : (
        <ProgressRing fraction={feature.total === 0 ? 0 : feature.done / feature.total} />
      )}
    </span>
  );
}

/** "done · 2 plans": the state word in `--foreground`, the count after it muted. */
function FeatureStatus({ status }: { status: string }) {
  const [word, ...rest] = status.split(" · ");
  return (
    <span className="kickoff-feature__status">
      <b>{word}</b>
      {rest.length > 0 && ` · ${rest.join(" · ")}`}
    </span>
  );
}

/** The picked feature where the hint was: its mark, slug, status and Change. */
export function PickedFeature({
  feature,
  onChange,
}: {
  feature: FollowUpFeature;
  onChange: () => void;
}) {
  return (
    <span className="kickoff-picked">
      <FeatureMark feature={feature} />
      <span className="kickoff-feature__id">
        <Untrusted text={feature.name} />
      </span>
      <FeatureStatus status={feature.status} />
      <button type="button" className="kickoff-picked__change" onClick={onChange}>
        Change
      </button>
    </span>
  );
}

/** Where the chip would be while a Follow-up has no feature: the way back to the picker. */
export function PickFeature({ onPick }: { onPick: () => void }) {
  return (
    <button type="button" className="kickoff-picked kickoff-pick" onClick={onPick}>
      Pick a feature
    </button>
  );
}

function step(active: number, key: string, last: number): number | null {
  switch (key) {
    case "ArrowDown":
      return Math.min(last, active + 1);
    case "ArrowUp":
      return Math.max(0, active - 1);
    case "Home":
      return 0;
    case "End":
      return last;
    default:
      return null;
  }
}

/** Closes the listbox on a pointer press anywhere outside it. */
function useCloseOutside(root: RefObject<HTMLElement | null>, onClose: () => void) {
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const press = (event: PointerEvent) => {
      if (!(event.target instanceof Node) || root.current?.contains(event.target)) return;
      close.current();
    };
    document.addEventListener("pointerdown", press);
    return () => document.removeEventListener("pointerdown", press);
  }, [root]);
}

/**
 * The feature a follow-up is for: a listbox floating under the kind switch, focused on open, one
 * option per feature with a metaplan. Arrows move without wrapping, Home and End jump, Enter or
 * Space picks, a click picks, Escape closes; the caller returns focus to what opened it.
 */
export function FeaturePicker({
  features,
  onPick,
  onClose,
}: {
  features: FollowUpFeature[];
  onPick: (feature: string) => void;
  onClose: () => void;
}) {
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(0);
  useEffect(() => root.current?.focus({ preventScroll: true }), []);
  useCloseOutside(root, onClose);
  const last = features.length - 1;
  const optionId = (index: number) => `${id}-${index}`;
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const next = step(active, event.key, last);
    if (next !== null) {
      event.preventDefault();
      if (last >= 0) setActive(next);
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
    } else if ((event.key === "Enter" || event.key === " ") && last >= 0) {
      event.preventDefault();
      onPick(features[active]!.name);
    }
  };
  return (
    <div
      ref={root}
      role="listbox"
      aria-label="Which feature?"
      aria-activedescendant={last >= 0 ? optionId(active) : undefined}
      tabIndex={0}
      className="kickoff-picker"
      onKeyDown={onKeyDown}
    >
      <p className="kickoff-picker__title" aria-hidden="true">
        Which feature is this for?
      </p>
      {last < 0 ? (
        <p className="kickoff-picker__empty">{EMPTY}</p>
      ) : (
        // role="none" keeps the list out of the tree so each option sits directly in the listbox.
        <ul role="none" className="kickoff-picker__list">
          {features.map((feature, index) => (
            <li
              key={feature.name}
              id={optionId(index)}
              role="option"
              aria-selected={index === active}
              aria-label={`${visibleControls(feature.name)}, ${feature.status}`}
              className="kickoff-feature"
              onPointerMove={() => setActive(index)}
              onClick={() => onPick(feature.name)}
            >
              <FeatureMark feature={feature} />
              <span className="kickoff-feature__id">
                <Untrusted text={feature.name} />
              </span>
              <FeatureStatus status={feature.status} />
            </li>
          ))}
        </ul>
      )}
      <p className="kickoff-picker__foot" aria-hidden="true">
        <span>
          <kbd>↑↓</kbd> choose · <kbd>↵</kbd> pick
        </span>
        <span>A done feature offers its open findings</span>
      </p>
    </div>
  );
}
