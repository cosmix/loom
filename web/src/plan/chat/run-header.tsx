import { useAtomValue } from "jotai/react";

import { formatClock } from "@/lib/format";
import { compactTokens } from "@/lib/tokens";
import type { SessionView } from "@/plan/api/schema-chat";
import { authoringAtom, isHolderAtom, presenceAtom } from "@/plan/state/atoms";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { BUSY_STATES } from "./chat-state-glyph";
import { turnsAtom } from "./model";
import { PressureHeader } from "./pressure/pressure-header";
import { Untrusted, visibleControls } from "./visible-controls";

/**
 * The run line under the head: who is authoring, who is watching, what the chat has cost. One
 * line that never wraps (chat-surface.css): the name segment is cut with an ellipsis first, each
 * separator belongs to the segment after it, and under 22rem the token count drops.
 */
export function RunHeader({ session }: { session: SessionView }) {
  const authoring = useAtomValue(authoringAtom);
  const presence = useAtomValue(presenceAtom);
  const isHolder = useAtomValue(isHolderAtom);
  const turns = useAtomValue(turnsAtom);
  if (session.kind === "pressure") return <PressureHeader session={session} />;
  const holder = authoring?.holder;
  const name = holder?.name ?? session.holder ?? "No one";
  const watchers =
    presence?.entries.filter((entry) => !entry.holder && entry.public_id !== holder?.public_id) ??
    [];
  const names = visibleControls(watchers.map((entry) => entry.name).join(", "));
  const total = turns.reduce(
    (sum, turn) => sum + turn.tokens.input + turn.tokens.cache_write + turn.tokens.output,
    0,
  );
  // Authoring presence clears on return before the next session snapshot arrives.
  const awayUntil = holder ? holder.frees_at : session.away_until;
  const activity = BUSY_STATES.has(session.state)
    ? "drafting"
    : session.state === "idle"
      ? "waiting"
      : null;
  return (
    <div className="chat-run-header chat-run-line">
      {activity && <ActivityGlyph kind={activity} />}
      <span className="chat-run-seg" data-cut>
        {awayUntil ? (
          <>
            <Untrusted text={name} /> is away · authoring frees at{" "}
            {formatClock(awayUntil).slice(0, 5)}
          </>
        ) : (
          <>
            {!isHolder && "Read-only · "}
            <Untrusted text={name} /> {isHolder ? "authoring" : "is authoring"}
          </>
        )}
      </span>
      <span className="chat-run-seg">
        <span aria-hidden="true">· </span>
        <span
          tabIndex={0}
          title={names}
          aria-label={`${watchers.length} watching${watchers.length ? `: ${names}` : ""}`}
        >
          {watchers.length} watching
        </span>
      </span>
      <span className="chat-run-seg chat-run-tokens">· {compactTokens(total)} tokens</span>
    </div>
  );
}
