import { useEffect, useRef, useState, type JSX, type RefObject } from "react";

import type { EditorHandle, EditorMode, SourceEditorHandle } from "./codemirror";
import { loadCodeMirror } from "./load";

export interface SourceEditorProps {
  path: string;
  text: string;
  readOnly?: boolean;
  onChange(text: string): void;
  onSave(): void;
  /** The mode the toolbar shows. The editor is built in it and follows it without a rebuild. */
  mode: EditorMode;
  /** Mod-click on a link in live preview; the raw href from the Markdown source. */
  onFollowLink?(href: string): void;
  /** 1-based lines marked in the gutter. */
  gutterMarks?: number[];
  /** Receives the live editor (null while loading or after unmount) for scroll and selection lookups. */
  handleRef?: RefObject<EditorHandle | null>;
}

type Phase = "loading" | "ready" | "failed";

/**
 * Owns the CodeMirror host ref and load phase for one source editor: builds the editor
 * on mount and on path or read-only change, then syncs text and gutter marks into it.
 */
function useSourceEditorHost(props: SourceEditorProps) {
  const { path, text, readOnly = false, gutterMarks, mode } = props;
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<SourceEditorHandle | null>(null);
  const latest = useRef(props);
  latest.current = props;
  const [phase, setPhase] = useState<Phase>("loading");
  useEffect(() => {
    let cancelled = false;
    let created: SourceEditorHandle | null = null;
    setPhase("loading");
    loadCodeMirror().then(
      (codemirror) => {
        const parent = host.current;
        if (cancelled || !parent) return;
        created = codemirror.createSourceEditor(parent, {
          doc: latest.current.text,
          readOnly,
          onChange: (next) => latest.current.onChange(next),
          onSave: () => latest.current.onSave(),
          mode: latest.current.mode,
          onFollowLink: (href) => latest.current.onFollowLink?.(href),
        });
        handle.current = created;
        if (latest.current.handleRef) latest.current.handleRef.current = created;
        setPhase("ready");
      },
      () => {
        if (!cancelled) setPhase("failed");
      },
    );
    return () => {
      cancelled = true;
      created?.destroy();
      if (handle.current === created) handle.current = null;
      if (latest.current.handleRef?.current === created) latest.current.handleRef.current = null;
    };
  }, [path, readOnly]);
  useEffect(() => {
    const editor = handle.current;
    if (phase === "ready" && editor && editor.getText() !== text) editor.setText(text);
  }, [phase, text]);
  useEffect(() => {
    const editor = handle.current;
    if (phase === "ready" && editor && editor.mode() !== mode) editor.setMode(mode);
  }, [phase, mode]);
  useEffect(() => {
    if (phase === "ready") handle.current?.setGutterMarks?.(gutterMarks ?? []);
  }, [phase, gutterMarks]);
  return { host, phase };
}

/**
 * The CodeMirror source surface for one document. The editor is rebuilt when the
 * path or read-only state changes, never when the opened hash moves after a save or the
 * mode changes (either would drop undo history, cursor and scroll). `text` is applied only when it differs
 * from what the editor holds, so typing never loops back through props.
 */
export function SourceEditor(props: SourceEditorProps): JSX.Element {
  const { host, phase } = useSourceEditorHost(props);
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {phase === "loading" ? (
        <p className="px-3 py-2 text-[13px] text-muted-foreground">Loading editor</p>
      ) : null}
      {phase === "failed" ? (
        <p role="alert" className="px-3 py-2 text-[13px] text-destructive">
          The editor failed to load. Reload the page to try again.
        </p>
      ) : null}
      <div ref={host} className="min-h-0 flex-1 overflow-auto" />
    </div>
  );
}
