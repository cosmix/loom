import { useAtomValue } from "jotai";
import { MessagesSquare } from "lucide-react";

import type { SessionView } from "@/plan/api/schema-chat";
import { STATE_NAME } from "@/plan/chat/chat-state-glyph";
import { sessionAtom } from "@/plan/chat/model";
import { ActivityGlyph, type ActivityKind } from "@/plan/ui/activity-glyphs";

// The names come from the chat panel's STATE_NAME; the glyphs are its activity glyphs.
const STATE_GLYPH: Record<SessionView["state"], ActivityKind | null> = {
  starting: "starting",
  idle: "waiting",
  working: "drafting",
  permission: "permission",
  ending: "drafting",
  ended: null,
  failed: null,
};

export function KickoffHead() {
  const state = useAtomValue(sessionAtom)?.state;
  const glyph = state ? STATE_GLYPH[state] : null;
  return (
    <div className="kickoff-head">
      <MessagesSquare aria-hidden="true" />
      <h2>New work</h2>
      <span className="kickoff-head__sub">· kickoff conversation</span>
      {state && (
        <span className="kickoff-pill" data-state={state}>
          {glyph && <ActivityGlyph kind={glyph} />}
          {STATE_NAME[state]}
        </span>
      )}
    </div>
  );
}
