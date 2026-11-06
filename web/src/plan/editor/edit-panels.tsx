import { useEffect, useRef, type JSX } from "react";

import { cn } from "cn";

import { Button } from "@/components/ui/button";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import type { EditorMode, FormatCommand } from "./codemirror";
import { FormatButtons } from "./edit-toolbar-controls";

const MODES: { mode: EditorMode; label: string }[] = [
  { mode: "preview", label: "Live preview" },
  { mode: "source", label: "Source" },
];

/** Two buttons, one pressed: Live preview draws the Markdown, Source shows it as typed. */
function ModeToggle(props: { mode: EditorMode; onMode(mode: EditorMode): void }): JSX.Element {
  return (
    <div
      role="group"
      aria-label="Editor mode"
      className="inline-flex h-7 items-stretch overflow-hidden rounded-md border border-hairline"
    >
      {MODES.map(({ mode, label }) => {
        const pressed = props.mode === mode;
        return (
          <button
            key={mode}
            type="button"
            aria-pressed={pressed}
            onClick={() => props.onMode(mode)}
            className={cn(
              "px-2.5 text-[0.8rem] transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring/50 focus-visible:ring-inset",
              "not-first:border-l not-first:border-hairline",
              pressed
                ? "bg-muted font-medium text-foreground"
                : "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
            )}
          >
            {label}
          </button>
        );
      })}
    </div>
  );
}

export function GonePanel(props: { what: string; text: string; onDiscard(): void }): JSX.Element {
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 px-4 py-3">
      <p role="alert" className="text-[13px]">
        This file was {props.what}; your text is kept
      </p>
      <div className="flex gap-2">
        <Button
          type="button"
          size="sm"
          data-look="secondary"
          className="plan-button"
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
  mode: EditorMode;
  onMode(mode: EditorMode): void;
  onFormat(command: FormatCommand): void;
  /** Read-only editor or the merge view: the formatting buttons do nothing there. */
  formatDisabled: boolean;
}): JSX.Element {
  return (
    <div className="flex flex-wrap items-start gap-x-2 gap-y-1.5 border-b border-hairline px-4 py-2">
      <DisabledReason reason={props.blockReason} onClick={props.onSave} size="sm">
        Save
      </DisabledReason>
      <Button
        type="button"
        size="sm"
        data-look="secondary"
        className="plan-button"
        onClick={props.onDone}
      >
        Done
      </Button>
      <span aria-hidden="true" className="mx-1 h-7 w-px self-start bg-hairline" />
      <ModeToggle mode={props.mode} onMode={props.onMode} />
      <FormatButtons disabled={props.formatDisabled} onFormat={props.onFormat} />
      {props.saving && (
        <span className="self-center text-[13px] text-muted-foreground">Saving</span>
      )}
    </div>
  );
}
