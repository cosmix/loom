import { useAtomValue } from "jotai/react";
import { useCallback, useMemo, useRef, useState, type JSX, type KeyboardEvent } from "react";
import { useNavigate } from "react-router";

import type { DocView } from "@/plan/api/schema-files";
import { AddToChat } from "@/plan/attachments";
import { resolveLink, toAddress } from "@/plan/files/address";
import { docChangedAtom, validationAtom } from "@/plan/files/atoms";
import { authorBlockReasonAtom, authoringAtom } from "@/plan/state/atoms";

import type { EditorHandle, EditorMode, FormatCommand, SourceEditorHandle } from "./codemirror";
import { draftNote, stageSpans } from "./edit-flow";
import { EditToolbar, LeaveChoice, Notes } from "./edit-panels";
import type { EditSession } from "./edit-session";
import { readEditorMode, writeEditorMode } from "./editor-mode";
import { editorProvenance } from "./editor-provenance";
import { MergePanel } from "./merge-panel";
import { useEditorSelectionAttachment } from "./selection-attachment";
import { SourceEditor } from "./source-editor";
import { errorLines, hasFindings, ValidationPanel } from "./validation-panel";

function useSourceAttachment(
  doc: DocView,
  openedHash: string,
  wrapper: { current: HTMLDivElement | null },
  handle: Handle,
) {
  const provenance = useMemo(
    () => editorProvenance(doc.path, doc.sections, stageSpans(doc)),
    [doc],
  );
  return useEditorSelectionAttachment(wrapper, handle, (selection, lines, text) => {
    const draft = provenance(selection, lines, text);
    return draft && draft.kind === "source" ? { ...draft, hash: openedHash } : draft;
  });
}

/** The source surface's Add to chat, in its own component to keep `EditBody` short. */
function SourceAddToChat(props: {
  doc: DocView;
  /** The hash the editor opened on; the draft's anchor. */
  hash: string;
  wrapper: { current: HTMLDivElement | null };
  handle: Handle;
}): JSX.Element | null {
  const attachment = useSourceAttachment(props.doc, props.hash, props.wrapper, props.handle);
  if (!attachment.draft || !attachment.rect) return null;
  return <AddToChat draft={attachment.draft} rect={attachment.rect} onAdded={attachment.clear} />;
}

function StaleNote(props: { onCompare(): void }): JSX.Element {
  return (
    <p role="note" className="px-4 py-1 text-[13px] text-muted-foreground">
      Changed on disk ·{" "}
      <button type="button" className="underline" onClick={props.onCompare}>
        Compare
      </button>
    </p>
  );
}

type Handle = { current: EditorHandle | null };

/**
 * Mod-click on a link in live preview. Documents and anchors go through the router, so the
 * leave guard asks first when text is unsaved; external links open in a new tab, which leaves
 * the page (and its text) alone; anything else does nothing.
 */
function useFollowLink(path: string): (href: string) => void {
  const navigate = useNavigate();
  return useCallback(
    (href: string) => {
      const link = resolveLink(path, href);
      if (link.kind === "doc") navigate(toAddress(link.path, link.anchor));
      else if (link.kind === "anchor") navigate(toAddress(path, link.anchor));
      else if (link.kind === "external") window.open(link.href, "_blank", "noopener,noreferrer");
    },
    [path, navigate],
  );
}

/**
 * The editor mode and the toolbar's routes into the editor. The mode lives here, read once
 * from storage, so the source editor never reads it and a rebuild keeps what the toolbar shows.
 */
function useEditorControls(path: string, handle: Handle) {
  const [mode, setMode] = useState<EditorMode>(() => readEditorMode());
  const onMode = useCallback((next: EditorMode) => {
    writeEditorMode(next);
    setMode(next);
  }, []);
  const onFormat = useCallback(
    (command: FormatCommand) => {
      const editor = handle.current;
      if (editor && "format" in editor) (editor as SourceEditorHandle).format(command);
    },
    [handle],
  );
  return { mode, onMode, onFormat, onFollowLink: useFollowLink(path) };
}

/** True when the file changed on disk after the opened hash and no merge is open. */
function useStaleSource(path: string, openedHash: string, merging: boolean): boolean {
  const changedHash = useAtomValue(docChangedAtom)[path];
  return typeof changedHash === "string" && changedHash !== openedHash && !merging;
}

/** The validation the panel shows: this save's result, else the watched one. */
function useEditValidation(path: string, own: EditSession["flow"]["state"]["validation"]) {
  const watched = useAtomValue(validationAtom)[path] ?? null;
  const validation = own ?? watched;
  const marks = useMemo(() => errorLines(validation), [validation]);
  return { validation, marks };
}

/** Escape leaves edit mode unless a handler already took it or the search panel is open. */
function editKeyHandler(wrapper: { current: HTMLDivElement | null }, session: EditSession) {
  return (event: KeyboardEvent) => {
    const searching = wrapper.current?.querySelector(".cm-search") != null;
    if (event.key === "Escape" && !event.defaultPrevented && !searching) session.done();
  };
}

function EditNotices(props: { session: EditSession; stale: boolean }): JSX.Element {
  const { flow, canAuthor } = props.session;
  const holder = useAtomValue(authoringAtom)?.holder;
  return (
    <>
      {flow.state.status === "error" && (
        <p role="alert" className="px-4 py-1 text-[13px] text-destructive">
          {flow.state.error}
        </p>
      )}
      {props.stale && <StaleNote onCompare={() => void flow.compare()} />}
      <Notes
        lines={[
          flow.compareError,
          canAuthor
            ? null
            : `${holder?.name ?? "Another browser"} took authoring. Your text is kept as a draft.`,
          flow.restored ? "Restored unsaved text" : null,
          draftNote(flow.draftResult),
        ]}
      />
    </>
  );
}

function EditSurface(props: {
  doc: DocView;
  session: EditSession;
  marks: number[];
  handle: Handle;
  mode: EditorMode;
  onFollowLink(href: string): void;
}): JSX.Element {
  const { session, handle } = props;
  const { state } = session.flow;
  const save = () => session.flow.save(false);
  if (state.currentText !== null) {
    return (
      <MergePanel
        key={state.openedHash}
        left={state.currentText}
        right={state.text}
        onChange={session.flow.change}
        onSave={save}
        readOnly={!session.canAuthor}
        handleRef={handle}
      />
    );
  }
  return (
    <SourceEditor
      path={props.doc.path}
      text={state.text}
      readOnly={!session.canAuthor}
      onChange={session.flow.change}
      onSave={save}
      gutterMarks={props.marks}
      mode={props.mode}
      onFollowLink={props.onFollowLink}
      handleRef={handle}
    />
  );
}

/** Save, Done, the mode toggle and the formatting buttons, wired to the session and editor. */
function EditBodyToolbar(props: {
  session: EditSession;
  controls: ReturnType<typeof useEditorControls>;
}): JSX.Element {
  const { session, controls } = props;
  const { state } = session.flow;
  const blockReason = useAtomValue(authorBlockReasonAtom);
  return (
    <EditToolbar
      blockReason={blockReason}
      saving={state.status === "saving"}
      onSave={() => session.flow.save(false)}
      onDone={session.done}
      mode={controls.mode}
      onMode={controls.onMode}
      onFormat={controls.onFormat}
      // Read-only, or the merge view (a different surface) is showing.
      formatDisabled={!session.canAuthor || state.currentText !== null}
    />
  );
}

/** The toolbar, notices, validation panel and the source or merge surface. */
export function EditBody({ doc, session }: { doc: DocView; session: EditSession }): JSX.Element {
  const { flow } = session;
  const { state } = flow;
  const wrapper = useRef<HTMLDivElement>(null);
  const handle = useRef<EditorHandle | null>(null);
  const { validation, marks } = useEditValidation(doc.path, state.validation);
  const stale = useStaleSource(doc.path, state.openedHash, state.currentText !== null);
  const onKeyDown = editKeyHandler(wrapper, session);
  const controls = useEditorControls(doc.path, handle);

  return (
    // The key handler only watches Escape from inside the editor; it is not a control.
    <div
      ref={wrapper}
      onKeyDown={onKeyDown}
      className="flex min-h-0 flex-1 flex-col"
      data-edit-mode
    >
      <EditBodyToolbar session={session} controls={controls} />
      {session.asking && (
        <LeaveChoice
          onSave={session.saveAndClose}
          onDiscard={session.discardAndLeave}
          onKeep={session.keepEditing}
        />
      )}
      <EditNotices session={session} stale={stale} />
      {hasFindings(validation) && (
        <ValidationPanel
          validation={validation}
          onJump={(line) => handle.current?.scrollToLine?.(line)}
          onSaveWithErrors={state.status === "invalid" ? () => flow.save(true) : undefined}
        />
      )}
      <EditSurface
        doc={doc}
        session={session}
        marks={marks}
        handle={handle}
        mode={controls.mode}
        onFollowLink={controls.onFollowLink}
      />
      <SourceAddToChat doc={doc} hash={state.openedHash} wrapper={wrapper} handle={handle} />
    </div>
  );
}
