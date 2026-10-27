import { useEffect, useRef, useState, type JSX, type RefObject } from "react";

import type { EditorHandle } from "./codemirror";
import { loadCodeMirror } from "./load";

type Props = {
  /** The file as it is now, read-only. */
  left: string;
  /** The user's text, editable; changes are reported as they happen. */
  right: string;
  onChange(text: string): void;
  /** Ctrl-S / Cmd-S on the editable side; same action as the Save button. */
  onSave(): void;
  /** Receives the editable side's handle (null before load and after unmount). */
  handleRef?: RefObject<EditorHandle | null>;
};

/**
 * Two-sided merge: the current file on the left, the user's text on the right. The view
 * is built once per mount; the parent remounts it (by key) when the left side changes.
 */
export function MergePanel({ left, right, onChange, onSave, handleRef }: Props): JSX.Element {
  const host = useRef<HTMLDivElement>(null);
  const latest = useRef(onChange);
  latest.current = onChange;
  const saveRef = useRef(onSave);
  saveRef.current = onSave;
  // left and right seed the view once; later edits live inside it.
  const seed = useRef({ left, right });
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    let handle: EditorHandle | null = null;
    loadCodeMirror().then(
      (codemirror) => {
        const parent = host.current;
        if (cancelled || !parent) return;
        handle = codemirror.createMergeView(parent, {
          ...seed.current,
          onChange: (text) => latest.current(text),
          onSave: () => saveRef.current(),
        });
        if (handleRef) handleRef.current = handle;
      },
      () => {
        if (!cancelled) setFailed(true);
      },
    );
    return () => {
      cancelled = true;
      handle?.destroy();
      if (handleRef && handleRef.current === handle) handleRef.current = null;
    };
  }, [handleRef]);

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <p className="px-4 py-2 text-[13px] text-muted-foreground">
        Left is the file as it is now; right is your text. Move changes across, edit the right side,
        then save.
      </p>
      {failed && (
        <p role="alert" className="px-4 py-2 text-sm text-destructive">
          The editor failed to load. Reload the page to try again.
        </p>
      )}
      <div ref={host} className="min-h-0 flex-1 overflow-auto" />
    </div>
  );
}
