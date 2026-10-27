import { useAtomValue } from "jotai/react";
import { useMemo, useRef, type JSX, type KeyboardEvent } from "react";

import type { DocView } from "@/plan/api/schema-files";
import { AddToChat } from "@/plan/attachments";
import { docChangedAtom, validationAtom } from "@/plan/files/atoms";
import { authorBlockReasonAtom, authoringAtom } from "@/plan/state/atoms";

import type { EditorHandle } from "./codemirror";
import { draftNote, stageSpans } from "./edit-flow";
import { EditToolbar, LeaveChoice, Notes } from "./edit-panels";
import type { EditSession } from "./edit-session";
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
  return useEditorSelectionAttachment(wrapper, handle, (selection, lines) => {
    const draft = provenance(selection, lines);
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
        <p role="alert" className="px-4 py-1 text-sm text-destructive">
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
      handleRef={handle}
    />
  );
}

/** The toolbar, notices, validation panel and the source or merge surface. */
export function EditBody({ doc, session }: { doc: DocView; session: EditSession }): JSX.Element {
  const { flow } = session;
  const { state } = flow;
  const wrapper = useRef<HTMLDivElement>(null);
  const handle = useRef<EditorHandle | null>(null);
  const blockReason = useAtomValue(authorBlockReasonAtom);
  const { validation, marks } = useEditValidation(doc.path, state.validation);
  const stale = useStaleSource(doc.path, state.openedHash, state.currentText !== null);
  const onKeyDown = editKeyHandler(wrapper, session);

  return (
    // The key handler only watches Escape from inside the editor; it is not a control.
    <div
      ref={wrapper}
      onKeyDown={onKeyDown}
      className="flex min-h-0 flex-1 flex-col"
      data-edit-mode
    >
      <EditToolbar
        blockReason={blockReason}
        saving={state.status === "saving"}
        onSave={() => flow.save(false)}
        onDone={session.done}
      />
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
      <EditSurface doc={doc} session={session} marks={marks} handle={handle} />
      <SourceAddToChat doc={doc} hash={state.openedHash} wrapper={wrapper} handle={handle} />
    </div>
  );
}
