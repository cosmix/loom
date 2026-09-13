import { ArrowLeftIcon, ArrowRightIcon, KeyboardIcon } from "lucide-react";
import { useEffect, useState } from "react";
import type { FocusEvent, ReactElement, ReactNode } from "react";

import type { TerminalMode, TerminalPhase } from "@/components/terminal/use-terminal";
import { useMediaQuery } from "@/lib/use-media-query";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

/// How long the knob takes to slide from one cell to the other. The state
/// words underneath only swap once it arrives, so this and the CSS custom
/// property `--terminal-key-slide` in terminal-controls.css must match.
const SLIDE_MS = 250;

/// How long the knob's own label keeps naming the action just taken before
/// it flips to name the next one, once the slide has already landed.
const HOLD_MS = 300;

/// The verb the knob carries in each direction, shared between its rider
/// and the invisible sizer that reserves room for it in the cell below.
const TAKE_CONTROL: ReactNode = (
  <>
    take control
    <ArrowRightIcon aria-hidden="true" />
  </>
);
const RELEASE: ReactNode = (
  <>
    <ArrowLeftIcon aria-hidden="true" />
    release
  </>
);

/// The word the key shows on its left: the mode while live, else the phase.
function stateWord(mode: TerminalMode, phase: TerminalPhase): string {
  if (phase === "live") return mode === "view" ? "viewing" : "controlling";
  return phase;
}

/// A cell's state label: shown once the knob has finished uncovering it,
/// faded out and hidden from assistive tech the rest of the time.
function Label({ active, children }: { active: boolean; children: ReactNode }): ReactElement {
  return (
    <span
      className="terminal-key-label"
      data-kind="state"
      data-active={active}
      aria-hidden={!active}
    >
      {children}
    </span>
  );
}

/// An invisible twin of the knob's rider, stacked with the state label in
/// the same cell so the column stays wide enough for whichever verb text
/// the knob shows while parked over it — the knob is absolutely positioned
/// and never itself contributes to the cell's width.
function Sizer({ children }: { children: ReactNode }): ReactElement {
  return (
    <span className="terminal-key-label terminal-key-sizer" aria-hidden="true">
      {children}
    </span>
  );
}

/// One instrument for the header: a two-cell sliding toggle of fixed width.
/// The knob is the verb tile itself and carries its own label. On a click
/// the destination cell's word vanishes at once, the knob slides across
/// still carrying the label the user clicked, and the cell it uncovers
/// shows its word only once the knob has landed. The knob's own label then
/// lingers for another beat before it flips to name the next action, so the
/// action just taken stays readable rather than changing the instant the
/// knob stops moving. When the phase cannot be switched it turns into a
/// plain indicator and drops both cells.
export function ModeKey({
  mode,
  phase,
  switchable,
  onMode,
}: {
  mode: TerminalMode;
  phase: TerminalPhase;
  switchable: boolean;
  onMode: (mode: TerminalMode) => void;
}): ReactElement {
  const control = mode === "control";
  const word = stateWord(mode, phase);
  const reduced = useMediaQuery("(prefers-reduced-motion: reduce)");
  const [settled, setSettled] = useState<TerminalMode>(mode);
  const [rider, setRider] = useState<TerminalMode>(mode);

  // `settled` lags `mode` by the slide, so the cell the knob uncovers shows
  // its word only once the knob has landed. `rider` lags it further, by the
  // slide plus a hold, so the knob's own label keeps naming the action just
  // taken for a beat after it stops moving before it flips to the next one.
  // A click mid-slide flips `mode` back before either timer fires; the
  // cleanup below cancels both pending swaps, so neither word changes.
  useEffect(() => {
    if (reduced) {
      setSettled(mode);
      setRider(mode);
      return;
    }
    const settleTimer = setTimeout(() => setSettled(mode), SLIDE_MS);
    const riderTimer = setTimeout(() => setRider(mode), SLIDE_MS + HOLD_MS);
    return () => {
      clearTimeout(settleTimer);
      clearTimeout(riderTimer);
    };
  }, [mode, reduced]);

  // The key is the dialog's first tabbable, so the dialog's auto-focus lands
  // here on open. A tooltip that opened on focus would then sit above the
  // dialog as the topmost dismissable layer and swallow the first Esc; the
  // tooltip is a hover affordance and the switch is already named for
  // assistive tech, so focus never opens it (Radix skips a prevented event).
  const onFocus = (event: FocusEvent<HTMLButtonElement>) => event.preventDefault();
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          className="terminal-key"
          role="switch"
          aria-checked={control}
          aria-label="Take control"
          disabled={!switchable}
          data-mode={mode}
          data-phase={phase}
          data-settled={settled}
          data-rider={rider}
          onClick={() => onMode(control ? "view" : "control")}
          onFocus={onFocus}
        >
          {switchable ? (
            <>
              <span className="terminal-key-cell">
                <Sizer>{TAKE_CONTROL}</Sizer>
                <Label active={mode === "control" && settled === "control"}>
                  <KeyboardIcon aria-hidden="true" />
                  {control ? word : "controlling"}
                </Label>
              </span>
              <span className="terminal-key-cell">
                <Label active={mode === "view" && settled === "view"}>
                  <span className="terminal-dot" aria-hidden="true" />
                  {control ? "viewing" : word}
                </Label>
                <Sizer>{RELEASE}</Sizer>
              </span>
              <span className="terminal-key-knob" aria-hidden="true">
                <span key={rider} className="terminal-key-rider">
                  {rider === "control" ? RELEASE : TAKE_CONTROL}
                </span>
              </span>
            </>
          ) : (
            <span className="terminal-key-state">
              <span className="terminal-dot" aria-hidden="true" />
              <span key={word} className="terminal-key-word">
                {word}
              </span>
            </span>
          )}
        </button>
      </TooltipTrigger>
      <TooltipContent>
        {control
          ? "Back to viewing — keys stop reaching the agent"
          : "Keystrokes reach a running autonomous agent"}
      </TooltipContent>
    </Tooltip>
  );
}
