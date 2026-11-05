import { useAtomValue } from "jotai/react";

import type { Item, SessionView } from "@/plan/api/schema-chat";
import { userWrote } from "./closing";
import { pendingPermissionsAtom, turnsAtom } from "./model";
import { panelSessionAtom } from "./session-home";
import { Untrusted } from "./visible-controls";

type Permission = Extract<Item, { type: "permission" }>;

/** The words the head's sub-line shows. Idle and no session draw nothing. */
const VISIBLE_WORDS: Partial<Record<SessionView["state"], string>> = {
  starting: "Starting",
  working: "Working",
  permission: "Needs your answer",
  ending: "Wrapping up",
  ended: "Ended",
  failed: "Stopped",
};

/** Whether the sub-line draws any words for this session (idle and none draw nothing). */
export function hasStateWords(session: SessionView | null): boolean {
  return session !== null && VISIBLE_WORDS[session.state] !== undefined;
}

/**
 * What the polite live region announces: the state and its stable facts only. Clocks and tool
 * summaries stay out, or the region would re-announce every few seconds for the whole turn. An idle
 * chat announces nothing: the composer is the cue.
 */
function stateWords(session: SessionView | null, pending: Permission[], writes: boolean): string {
  if (!session) return "No chat";
  switch (session.state) {
    case "starting":
      return `Starting session · ${session.name} · ${session.model}`;
    case "idle":
      return "";
    case "working":
      return "Working";
    case "permission":
      return pending.length > 1
        ? `Needs ${pending.length} decisions: ${pending.map((item) => item.tool).join(", ")}`
        : `Needs a decision${pending[0] ? `: ${pending[0].tool}` : ""}`;
    case "ending":
      // A chat the user never wrote to is torn down with no closing write to wait for.
      return writes
        ? "Writing final notes into the documents · This can take up to 10 minutes."
        : "Ending the chat";
    case "ended":
    case "failed":
      // The visible words and the failure notice carry these; ChatAnnouncer says them aloud.
      return "";
  }
}

/**
 * States whose announcement belongs to ChatAnnouncer, which stays mounted when the panel does
 * not (a closed sheet below 1280px). The polite region here leaves them out, so each is
 * announced once, and the permission words on the line are not hidden behind a duplicate.
 */
function announcedElsewhere(session: SessionView | null): boolean {
  const state = session?.state;
  return state === "permission" || state === "ended" || state === "failed";
}

/**
 * The head's sub-line: the state in a few words. It draws no roundel (the pane's one roundel is
 * the transcript's, on what is working). `hideWords` hides the words from browse mode where the
 * sr-only region announces them.
 */
function StateWords({ state, hideWords }: { state: SessionView["state"]; hideWords: boolean }) {
  const words = VISIBLE_WORDS[state];
  if (!words) return null;
  return (
    <span className="chat-state-row" aria-hidden={hideWords || undefined}>
      {state === "permission" && (
        <span aria-hidden="true" className="chat-state-glyph text-(--tone-warning)">
          ⚠
        </span>
      )}
      <span className="chat-state-words">{words}</span>
    </span>
  );
}

export function StateLine() {
  const session = useAtomValue(panelSessionAtom);
  const pending = useAtomValue(pendingPermissionsAtom);
  const turns = useAtomValue(turnsAtom);
  const words = stateWords(session, pending, userWrote(turns));
  const elsewhere = announcedElsewhere(session);
  return (
    <div
      className="chat-state-line"
      data-kind={session?.kind}
      data-state={session?.state ?? "none"}
    >
      <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
        <Untrusted text={elsewhere ? "" : words} />
      </div>
      {session && <StateWords state={session.state} hideWords={!elsewhere} />}
    </div>
  );
}
