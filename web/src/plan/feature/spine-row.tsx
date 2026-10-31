import { ChevronRight } from "lucide-react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import type { NameStyle, SpineChild, SpineGroup } from "@/plan/feature/spine-model";
import { STEP_MARK_GLYPHS } from "@/plan/ui/glyphs";

/** A document's name as its kind needs it: Markdown phrasing, a slug in mono, or plain text. */
export function Name({ name, style }: { name: string; style: NameStyle }) {
  if (style === "markdown") return <SentenceText text={name} />;
  const text = <Untrusted text={name} />;
  return style === "mono" ? <span className="plan-spine__mono">{text}</span> : text;
}

function ChildRow({ child, pathname }: { child: SpineChild; pathname: string }) {
  const Icon = child.icon;
  const body = (
    <>
      {/* The spaces keep the spans apart in the link's accessible name ("01 Name", "slug done"). */}
      {child.number !== null && (
        <>
          <span className="plan-spine__number">
            <Untrusted text={child.number} />
          </span>{" "}
        </>
      )}
      <span className="plan-spine__name">
        <Name name={child.name} style={child.style} />
      </span>
      {child.note !== null && (
        <>
          {" "}
          <span className="plan-status plan-fine">
            {Icon && <Icon aria-hidden="true" />}
            <Untrusted text={child.note} />
          </span>
        </>
      )}
    </>
  );
  if (child.to === null) return <li className="plan-spine__child">{body}</li>;
  return (
    <li>
      <Link
        to={child.to}
        aria-current={pathname === child.to ? "page" : undefined}
        className="plan-spine__link plan-spine__child"
      >
        {body}
      </Link>
    </li>
  );
}

function GroupHead({ group, selected }: { group: SpineGroup; selected: boolean }) {
  const Mark = STEP_MARK_GLYPHS[group.mark];
  return (
    <Link
      to={group.to}
      aria-current={selected ? "page" : undefined}
      className="plan-spine__link plan-spine__head"
    >
      <Mark className="plan-spine__mark" aria-hidden="true" />
      <span className="plan-spine__name">{group.label}</span>{" "}
      {group.count !== null ? (
        <>
          <span className="plan-fine">
            <Untrusted text={group.unit === null ? group.count : `${group.count} ${group.unit}`} />
          </span>
          {/* A shown unit ("6 done") already says it; the word is read only beside a bare count. */}
          {group.unit === null && <span className="sr-only">, {group.word}</span>}
        </>
      ) : (
        <span className="plan-fine">{group.word}</span>
      )}
    </Link>
  );
}

/** One group: its mark, name and count, and its documents while it is expanded. */
export function GroupRow({
  group,
  pathname,
  here,
  open,
  onToggle,
}: {
  group: SpineGroup;
  pathname: string;
  /** The route is inside this group: it stays expanded, with nothing to collapse. */
  here: boolean;
  open: boolean;
  onToggle: () => void;
}) {
  const hasChildren = group.children.length > 0;
  return (
    <li className="plan-spine__group" data-mark={group.mark} data-here={here ? "" : undefined}>
      <div className="plan-spine__row">
        {/* A group with an overview child gives the current mark to that child, which shares its target. */}
        <GroupHead group={group} selected={pathname === group.to && !group.overview} />
        {hasChildren && !here && (
          <Button
            type="button"
            size="sm"
            data-look="quiet"
            className="plan-button plan-spine__disclose"
            aria-expanded={open}
            aria-label={`${open ? "Collapse" : "Expand"} ${group.label}`}
            onClick={onToggle}
          >
            <ChevronRight aria-hidden="true" />
          </Button>
        )}
      </div>
      {hasChildren && open && (
        <ul className="plan-spine__children">
          {group.children.map((child) => (
            <ChildRow key={child.key} child={child} pathname={pathname} />
          ))}
        </ul>
      )}
    </li>
  );
}
