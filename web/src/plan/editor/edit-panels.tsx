import { useEffect, useRef, type JSX } from "react";

import { Button } from "@/components/ui/button";
import { DisabledReason } from "@/plan/ui/disabled-reason";

export function GonePanel(props: { what: string; text: string; onDiscard(): void }): JSX.Element {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 px-4 py-3">
      <p role="alert" className="text-[13px]">
        This file was {props.what}; your text is kept
      </p>
      <div className="flex gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={() => void navigator.clipboard?.writeText(props.text)}
        >
          Copy
        </Button>
        <Button type="button" variant="destructive" size="sm" onClick={props.onDiscard}>
          Discard
        </Button>
      </div>
      <textarea
        readOnly
        aria-label="Your kept text"
        value={props.text}
        className="min-h-0 flex-1 resize-none rounded border p-2 font-mono text-xs"
      />
    </div>
  );
}

export function Notes(props: { lines: (string | null)[] }): JSX.Element {
  return (
    <>
      {props.lines.map((line) =>
        line ? (
          <p key={line} role="note" className="px-4 py-1 text-[13px] text-muted-foreground">
            {line}
          </p>
        ) : null,
      )}
    </>
  );
}

/** Moves focus into the choice while it is open and gives it back to where it came from. */
function useFocusFirstButton() {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement;
    ref.current?.querySelector("button")?.focus();
    return () => {
      if (previous instanceof HTMLElement && previous.isConnected) previous.focus();
    };
  }, []);
  return ref;
}

export function LeaveChoice(props: {
  onSave(): void;
  onDiscard(): void;
  onKeep(): void;
}): JSX.Element {
  const ref = useFocusFirstButton();
  return (
    <div
      ref={ref}
      role="alertdialog"
      aria-label="Unsaved changes"
      className="flex flex-wrap gap-2 px-4 py-2"
    >
      <span className="text-[13px]">You have unsaved changes.</span>
      <Button type="button" size="sm" onClick={props.onSave}>
        Save and close
      </Button>
      <Button type="button" variant="destructive" size="sm" onClick={props.onDiscard}>
        Discard
      </Button>
      <Button type="button" variant="ghost" size="sm" onClick={props.onKeep}>
        Keep editing
      </Button>
    </div>
  );
}

export function EditToolbar(props: {
  blockReason: string | null;
  saving: boolean;
  onSave(): void;
  onDone(): void;
}): JSX.Element {
  return (
    <div className="flex flex-wrap items-start gap-2 border-b border-hairline px-4 py-2">
      <DisabledReason reason={props.blockReason} onClick={props.onSave} size="sm">
        Save
      </DisabledReason>
      <Button type="button" variant="outline" size="sm" onClick={props.onDone}>
        Done
      </Button>
      {props.saving && (
        <span className="self-center text-[13px] text-muted-foreground">Saving</span>
      )}
    </div>
  );
}
