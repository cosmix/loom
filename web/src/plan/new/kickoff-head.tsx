import { useAtomValue } from "jotai";
import { CircleAlert } from "lucide-react";

import type { SessionView } from "@/plan/api/schema-chat";
import type { KickoffSize } from "@/plan/api/schema-kickoff";
import { STATE_NAME } from "@/plan/chat/chat-state-glyph";
import { sessionAtom } from "@/plan/chat/model";
import { Untrusted } from "@/plan/chat/visible-controls";
import { kickoffAtom } from "@/plan/state/atoms";
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

const KIND_WORD: Record<KickoffSize, string> = { feature: "Feature", plan: "Standalone plan" };

/** The size the conversation settled, else the one the kickoff was started as. */
function kindOf(size: KickoffSize | null, offer: string | null): KickoffSize | null {
  if (size !== null) return size;
  if (offer === "kickoff-small") return "plan";
  return offer === "kickoff" ? "feature" : null;
}

/**
 * The live kickoff's page head, in the slot the empty form's head holds: the kind of work as the
 * eyebrow, the work's name (else "Untitled") as the one heading, and the session's state.
 */
export function KickoffHead() {
  const session = useAtomValue(sessionAtom);
  const view = useAtomValue(kickoffAtom);
  const shape = view && session && view.session === session.id ? view.shape : null;
  const state = session?.state;
  const glyph = state ? STATE_GLYPH[state] : null;
  const kind = kindOf(shape?.size ?? null, session?.offer ?? null);
  return (
    <header className="kickoff-title">
      <p className="plan-eyebrow">{kind ? KIND_WORD[kind] : "Kind not decided"}</p>
      <div className="kickoff-title__row">
        <h1 className="plan-page-title">
          {shape?.name ? <Untrusted text={shape.name} /> : "Untitled"}
        </h1>
        {state && (
          <span className="kickoff-pill plan-status" data-state={state}>
            {glyph && <ActivityGlyph kind={glyph} />}
            {state === "failed" && (
              <CircleAlert aria-hidden="true" className="kickoff-pill__alert" />
            )}
            {STATE_NAME[state]}
          </span>
        )}
      </div>
    </header>
  );
}
