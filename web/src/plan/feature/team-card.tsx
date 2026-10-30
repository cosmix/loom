import { CircleCheck } from "lucide-react";
import type { CSSProperties } from "react";

import { compactTokens } from "@/lib/tokens";
import type { TeammateStatus, TokenCounts } from "@/plan/api/schema-kickoff";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureColour } from "@/plan/home/feature-mark";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";

export function totalTokens(tokens: TokenCounts): number {
  return tokens.input + tokens.cache_write + tokens.cache_read + tokens.output;
}

/** A stable hue from the name, over the eight document colours. */
function avatarColour(name: string): CSSProperties {
  return { "--avatar": `var(--doc-${featureColour(name)})` } as CSSProperties;
}

function sectionMark(current: string | null): string | null {
  const match = current === null ? null : /section_(\d+)/.exec(current);
  return match ? `§${match[1]}` : null;
}

export function TeammateCard({ teammate }: { teammate: TeammateStatus }) {
  const { tokens } = teammate;
  const mark = sectionMark(teammate.current);
  const writing = teammate.state === "working";
  const [initial = ""] = Array.from(teammate.name);
  return (
    <li className="plan-team-card plan-surface">
      <span
        className="plan-team-card__avatar"
        style={avatarColour(teammate.name)}
        aria-hidden="true"
      >
        <Untrusted text={initial} />
      </span>
      <span className="plan-team-card__main">
        <span className="plan-team-card__name">
          <strong>
            <Untrusted text={teammate.name} />
          </strong>
          {mark && <span className="plan-team-card__mark">{mark}</span>}
        </span>
        <span className="plan-team-card__state">
          {writing ? (
            <ActivityGlyph kind="drafting" />
          ) : (
            <CircleCheck className="size-3.5" aria-hidden="true" />
          )}
          {writing ? "writing" : "drafted"}
        </span>
        <span className="sr-only">
          Current document: {teammate.current ? <Untrusted text={teammate.current} /> : "None"}
        </span>
        <span className="sr-only">
          {compactTokens(tokens.input)} in / {compactTokens(tokens.output)} out ·{" "}
          {compactTokens(tokens.cache_write)} cache write · {compactTokens(tokens.cache_read)} cache
          read
        </span>
      </span>
      <span className="plan-team-card__tokens">{compactTokens(totalTokens(tokens))}</span>
    </li>
  );
}
