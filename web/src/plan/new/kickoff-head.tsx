import { useAtomValue } from "jotai";

import type { SessionView } from "@/plan/api/schema-chat";
import type { KickoffSize } from "@/plan/api/schema-kickoff";
import { BackgroundLine, useBackgroundDisclosure } from "@/plan/chat/background-line";
import { BackgroundList } from "@/plan/chat/background-list";
import { sessionAtom } from "@/plan/chat/model";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { kindOf } from "@/plan/new/kickoff-state";
import { kickoffAtom } from "@/plan/state/atoms";

export const KIND_WORD: Record<KickoffSize, string> = {
  feature: "Feature",
  plan: "Standalone plan",
};

export const KIND_UNDECIDED = "Kind not decided";

/** The head's title before the conversation names the work. */
const NEW_TITLE: Record<KickoffSize, string> = {
  feature: "New feature",
  plan: "New standalone plan",
};

/**
 * The Hearth panel head's state words (chat/state-line.tsx), drawn here without its live region:
 * ChatAnnouncer already says the state, and the page keeps one status region for the writes.
 */
const STATE_WORDS: Partial<Record<SessionView["state"], string>> = {
  starting: "Starting",
  working: "Working",
  permission: "Needs your answer",
  ending: "Wrapping up",
  ended: "Ended",
  failed: "Stopped",
};

function StateWords({ state }: { state: SessionView["state"] }) {
  const words = STATE_WORDS[state];
  if (!words) return null;
  return (
    <span className="chat-state-row" data-state={state}>
      {state === "permission" && (
        <span aria-hidden="true" className="chat-state-glyph text-(--tone-warning)">
          ⚠
        </span>
      )}
      <span className="chat-state-words">{words}</span>
    </span>
  );
}

/**
 * The live kickoff's head, the Hearth chat head over a hairline: the work's name, else "New
 * feature" or "New standalone plan" by kind, else "New work", as the one heading, and under it
 * the state in a few words. Idle says nothing unless the session waits on background tasks, when
 * the panel head's background line and its list stand here too. It draws no busy sign: the
 * transcript's starting card or working turn carries the pane's one.
 */
export function KickoffHead() {
  const session = useAtomValue(sessionAtom);
  const view = useAtomValue(kickoffAtom);
  const background = useBackgroundDisclosure(session);
  const shape = view && session && view.session === session.id ? view.shape : null;
  const kind = kindOf(shape?.size ?? null, session?.offer ?? null);
  const fallback = kind ? NEW_TITLE[kind] : "New work";
  return (
    <header className="kickoff-head">
      <h1
        className="kickoff-head__title"
        title={shape?.name ? visibleControls(shape.name) : fallback}
      >
        {shape?.name ? <Untrusted text={shape.name} /> : fallback}
      </h1>
      {/* Nothing renders here while there is nothing to say: the line hides on `:empty`. */}
      <div className="kickoff-head__sub">
        {session && <StateWords state={session.state} />}
        <BackgroundLine disclosure={background} />
      </div>
      <BackgroundList disclosure={background} />
    </header>
  );
}
