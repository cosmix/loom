import { useAtomValue } from "jotai/react";

import { formatClock } from "@/lib/format";
import { compactTokens } from "@/lib/tokens";
import type { SessionView } from "@/plan/api/schema-chat";
import { authoringAtom, isHolderAtom, presenceAtom } from "@/plan/state/atoms";
import { turnsAtom } from "./model";
import { PressureHeader } from "./pressure/pressure-header";
import { Untrusted, visibleControls } from "./visible-controls";

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
  const total = turns.reduce(
    (sum, turn) => sum + turn.tokens.input + turn.tokens.cache_write + turn.tokens.output,
    0,
  );
  // Authoring presence clears on return before the next session snapshot arrives.
  const awayUntil = holder ? holder.frees_at : session.away_until;
  return (
    <div className="chat-run-header flex flex-wrap gap-x-1 px-3 text-xs text-muted-foreground">
      {awayUntil ? (
        <span>
          <Untrusted text={name} /> is away · authoring frees at{" "}
          {formatClock(awayUntil).slice(0, 5)}
        </span>
      ) : (
        <span>
          {!isHolder && "Read-only · "}
          <Untrusted text={name} /> {isHolder ? "authoring" : "is authoring"}
        </span>
      )}
      <span>·</span>
      <span
        tabIndex={0}
        title={visibleControls(watchers.map((entry) => entry.name).join(", "))}
        aria-label={`${watchers.length} watching${watchers.length ? `: ${visibleControls(watchers.map((entry) => entry.name).join(", "))}` : ""}`}
      >
        {watchers.length} watching
      </span>
      <span>· {compactTokens(total)} tokens</span>
    </div>
  );
}
