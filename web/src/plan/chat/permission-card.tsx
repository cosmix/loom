import { useAtomValue, useStore } from "jotai/react";
import { useEffect, useRef, useState, type JSX, type KeyboardEvent } from "react";

import type { Item } from "@/plan/api/schema-chat";
import { authoringAtom, isHolderAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { answerPermission } from "./commands";
import { pendingPermissionsAtom } from "./model";
import { PatchView } from "./patch-view";
import { Untrusted, visibleControls } from "./visible-controls";

type Permission = Extract<Item, { type: "permission" }>;
type Props = { item: Permission };

const editableSelector =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"])';

function isEditable(node: EventTarget | null): boolean {
  return node instanceof Element && node.closest(editableSelector) !== null;
}

function parsedInput(input: string): unknown {
  try {
    return JSON.parse(input);
  } catch {
    return input;
  }
}

function asFields(input: unknown): Record<string, unknown> {
  return input !== null && typeof input === "object" && !Array.isArray(input)
    ? (input as Record<string, unknown>)
    : {};
}

function replacementPatch(path: string, before: string, after: string): string {
  const lines = (text: string) => (text === "" ? [] : text.split("\n"));
  const oldLines = lines(before),
    newLines = lines(after);
  return [
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -${oldLines.length ? 1 : 0},${oldLines.length} +${newLines.length ? 1 : 0},${newLines.length} @@`,
    ...oldLines.map((line) => `-${line}`),
    ...newLines.map((line) => `+${line}`),
  ].join("\n");
}

type FileEdit = {
  path: string;
  patch: string;
  replaceAll: boolean;
  extras: Record<string, unknown>;
};

/**
 * The diff comes only from the fields the tool runs on (Edit: old_string and new_string,
 * Write: content), so the text the holder approves is the text that runs. Every other input
 * field, including a non-boolean replace_all, stays in `extras` and is shown as JSON.
 */
function fileEdit(tool: string, fields: Record<string, unknown>): FileEdit | null {
  if (tool !== "Edit" && tool !== "Write") return null;
  const text = (key: string) => (typeof fields[key] === "string" ? (fields[key] as string) : null);
  const pathKey = text("file_path") !== null ? "file_path" : "path";
  const path = text(pathKey);
  const afterKey = tool === "Write" ? "content" : "new_string";
  const after = text(afterKey);
  if (path === null || after === null) return null;
  const consumed = new Set([pathKey, afterKey]);
  let before = "";
  if (tool === "Edit") {
    before = text("old_string") ?? "";
    if (text("old_string") !== null) consumed.add("old_string");
  }
  const replaceAll = tool === "Edit" && fields.replace_all === true;
  if (tool === "Edit" && typeof fields.replace_all === "boolean") consumed.add("replace_all");
  const extras = Object.fromEntries(Object.entries(fields).filter(([key]) => !consumed.has(key)));
  return { path, patch: replacementPatch(path, before, after), replaceAll, extras };
}

function Mono({ text }: { text: string }): JSX.Element {
  return (
    <pre className="chat-tool-pre">
      <Untrusted text={text} />
    </pre>
  );
}

function FileEditBody({ edit }: { edit: FileEdit }): JSX.Element {
  const hasExtras = Object.keys(edit.extras).length > 0;
  return (
    <div className="space-y-2">
      <p className="chat-tool-pre">
        <Untrusted text={edit.path} />
      </p>
      {edit.replaceAll && (
        <p className="chat-decision-label">
          <Untrusted text="Replaces every occurrence" />
        </p>
      )}
      <PatchView patch={edit.patch} />
      {hasExtras && (
        <div className="space-y-1">
          <p className="plan-fine">Other input fields</p>
          <Mono text={JSON.stringify(edit.extras, null, 2)} />
        </div>
      )}
    </div>
  );
}

function PermissionBody({ item }: { item: Permission }): JSX.Element {
  const input = parsedInput(item.input);
  const fields = asFields(input);
  const edit = fileEdit(item.tool, fields);
  if (edit !== null) return <FileEditBody edit={edit} />;
  if (item.tool === "Bash")
    return <Mono text={typeof fields.command === "string" ? fields.command : item.input} />;
  return <Mono text={typeof input === "string" ? input : JSON.stringify(input, null, 2)} />;
}

function outcome(item: Permission): string {
  if (item.status === "allowed") return `Allowed: ${item.tool}`;
  switch (item.deny_reason) {
    case "timeout":
      return `Denied automatically after one hour: ${item.tool}`;
    case "interrupt":
      return `Denied when the turn was stopped: ${item.tool}`;
    case "session_ended":
    case "server_stopping":
      return "Denied: the session was ending";
    case "hook_error":
      return `Denied: the permission hook failed: ${item.tool}`;
    default:
      return `Denied: ${item.tool}${item.note ? ` · ${item.note}` : ""}`;
  }
}

function TimeLeft({ expiresAt }: { expiresAt: string }): JSX.Element {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, []);
  const seconds = Math.max(0, Math.ceil((Date.parse(expiresAt) - now) / 1000));
  const remaining = Number.isFinite(seconds)
    ? `${Math.floor(seconds / 60)}m ${seconds % 60}s left before automatic deny`
    : "Automatic deny after one hour";
  return <p className="plan-fine">{remaining}</p>;
}

/**
 * Moves focus to the pinned card once per request, only from a control inside the panel. The
 * assertive announcement is ChatAnnouncer's: this card is unmounted while the sheet is closed
 * (below 1280px the panel is a sheet), and the request must still be heard then.
 */
function usePinnedFocus(item: Permission) {
  const card = useRef<HTMLDivElement>(null);
  const focused = useRef<string | null>(null);
  useEffect(() => {
    if (focused.current === item.request_id) return;
    focused.current = item.request_id;
    const panel = card.current?.closest(
      'aside[aria-label="Chat"], [role="complementary"][aria-label="Chat"]',
    );
    // Never steal focus from a field the holder is typing in: the card answers on a bare
    // "y" or "n", so those keystrokes must keep landing in the composer. The assertive
    // announcement still tells the holder a decision is waiting.
    const active = document.activeElement;
    if (panel?.contains(active) && !isEditable(active)) card.current?.focus();
  }, [item.request_id]);
  return card;
}

function useAnswer(item: Permission) {
  const store = useStore();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sending = useRef(false);
  async function answer(allow: boolean, note: string | null = null) {
    if (sending.current || !store.get(isHolderAtom) || item.status !== "pending") return;
    sending.current = true;
    setBusy(true);
    setError(null);
    try {
      const result = await answerPermission(store, item.request_id, allow, note);
      if (!result.ok) setError(result.error ?? "The decision was refused");
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      sending.current = false;
      setBusy(false);
    }
  }
  return { answer, busy, error };
}

type Answer = (allow: boolean, note?: string | null) => Promise<void>;

function InitialButtons({
  answer,
  busy,
  onDeny,
}: {
  answer: Answer;
  busy: boolean;
  onDeny: () => void;
}): JSX.Element {
  const reason = busy ? "Sending" : null;
  return (
    <div className="chat-decision-actions">
      <DisabledReason reason={reason} onClick={() => void answer(true)}>
        Allow
      </DisabledReason>
      <DisabledReason reason={reason} onClick={onDeny}>
        Deny
      </DisabledReason>
    </div>
  );
}

function DenyNoteForm({ answer, busy }: { answer: Answer; busy: boolean }): JSX.Element {
  const [note, setNote] = useState("");
  const reason = busy ? "Sending" : null;
  return (
    <div className="space-y-2">
      <label className="chat-decision-label">
        Deny note
        <input
          autoFocus
          value={note}
          onChange={(event) => setNote(event.target.value)}
          className="chat-decision-input"
        />
      </label>
      <div className="chat-decision-actions">
        <DisabledReason reason={reason} onClick={() => void answer(false)}>
          Deny
        </DisabledReason>
        <DisabledReason
          reason={reason ?? (note.trim() ? null : "Type a note")}
          onClick={() => void answer(false, note)}
        >
          Deny with note
        </DisabledReason>
      </div>
    </div>
  );
}

function DecisionControls({
  denyOpen,
  setDenyOpen,
  answer,
  busy,
}: {
  denyOpen: boolean;
  setDenyOpen: (open: boolean) => void;
  answer: Answer;
  busy: boolean;
}): JSX.Element {
  if (denyOpen) return <DenyNoteForm answer={answer} busy={busy} />;
  return <InitialButtons answer={answer} busy={busy} onDeny={() => setDenyOpen(true)} />;
}

function DecisionFooter({
  error,
  ...controls
}: {
  denyOpen: boolean;
  setDenyOpen: (open: boolean) => void;
  answer: Answer;
  busy: boolean;
  error: string | null;
}): JSX.Element {
  const isHolder = useAtomValue(isHolderAtom);
  const holder = useAtomValue(authoringAtom)?.holder?.name ?? "the author";
  return (
    <>
      {isHolder ? (
        <DecisionControls {...controls} />
      ) : (
        <p className="plan-fine">
          <Untrusted text={`Waiting for ${holder} to decide`} />
        </p>
      )}
      {error && (
        <p role="alert" className="plan-fine">
          <Untrusted text={error} />
        </p>
      )}
    </>
  );
}

function usePermissionKeys(
  item: Permission,
  busy: boolean,
  answer: (allow: boolean) => Promise<void>,
  setDenyOpen: (open: boolean) => void,
) {
  const isHolder = useAtomValue(isHolderAtom);
  const pending = useAtomValue(pendingPermissionsAtom);
  return function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (!isHolder || busy || event.defaultPrevented || event.repeat) return;
    if (event.altKey || event.ctrlKey || event.metaKey || isEditable(event.target)) return;
    if (pending.length && pending[0].request_id !== item.request_id) return;
    if (event.key === "y") {
      event.preventDefault();
      void answer(true);
    }
    if (event.key === "n") {
      event.preventDefault();
      setDenyOpen(true);
    }
  };
}

function PendingPermission({ item }: Props): JSX.Element {
  const pending = useAtomValue(pendingPermissionsAtom);
  const [denyOpen, setDenyOpen] = useState(false);
  const { answer, busy, error } = useAnswer(item);
  const card = usePinnedFocus(item);
  const onKeyDown = usePermissionKeys(item, busy, answer, setDenyOpen);
  return (
    <div
      ref={card}
      role="region"
      aria-label={`Permission: ${visibleControls(item.tool)}`}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="chat-decision"
    >
      {pending.length > 1 && <p className="plan-fine">1 of {pending.length} decisions</p>}
      <p className="chat-decision-title">
        Needs a decision: <Untrusted text={item.tool} />
      </p>
      <PermissionBody item={item} />
      <TimeLeft expiresAt={item.expires_at} />
      <DecisionFooter
        denyOpen={denyOpen}
        setDenyOpen={setDenyOpen}
        answer={answer}
        busy={busy}
        error={error}
      />
    </div>
  );
}

export function PermissionCard({ item }: Props): JSX.Element {
  if (item.status === "pending") return <PendingPermission key={item.request_id} item={item} />;
  return (
    // A group, not a region: an answered row is history and would only crowd landmark navigation.
    <div
      role="group"
      aria-label={`Permission: ${visibleControls(item.tool)}`}
      className="chat-decision-outcome"
    >
      <Untrusted text={outcome(item)} />
    </div>
  );
}
