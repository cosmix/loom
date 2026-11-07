import { Archive, Check, ChevronRight, CircleDashed } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Link } from "react-router";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import { Button } from "@/components/ui/button";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { NOT_PRESSURE_TESTED, PRESSURE_TESTED } from "@/plan/files/heading-parts";
import type {
  GlyphKind,
  NameStyle,
  PlanPressure,
  RowGlyph,
  SpineChild,
  SpineGroup,
} from "@/plan/feature/spine-model";
import { SEAL_ICONS } from "@/plan/trace/settle-glyph";
import { STEP_MARK_GLYPHS } from "@/plan/ui/glyphs";

import "@/plan/trace/trace.css";

/** A document's name as its kind needs it: Markdown phrasing, a slug in mono, or plain text. */
export function Name({ name, style }: { name: string; style: NameStyle }) {
  if (style === "markdown") return <SentenceText text={name} />;
  const text = <Untrusted text={name} />;
  return style === "mono" ? <span className="plan-spine__mono">{text}</span> : text;
}

/** The row glyphs: the settle seals are the shared `SEAL_ICONS`, the rest are the row's own. */
const GLYPH_ICONS: Record<Exclude<GlyphKind, "running">, LucideIcon> = {
  ...SEAL_ICONS,
  done: Check,
  "not-written": CircleDashed,
  retired: Archive,
};

/**
 * The glyph that leads a row, chosen once in `rowGlyph` (`data-glyph` names the kind for the
 * stylesheet). It is hidden from assistive technology and its tooltip is the word; `GlyphWord`
 * reads the word after the row's name, so the link's name still starts with it. A running plan
 * draws the planner's roundel, the one the header pill draws (ui/DESIGN.md, Status).
 */
function GlyphMark({ glyph }: { glyph: RowGlyph }) {
  const Icon = glyph.kind === "running" ? null : GLYPH_ICONS[glyph.kind];
  return (
    <span
      className="plan-spine__glyph"
      data-glyph={glyph.kind}
      title={glyph.word ?? undefined}
      aria-hidden="true"
    >
      {Icon ? <Icon /> : <BusyRoundel busy size="100%" busyLabel="" idleLabel="" />}
    </span>
  );
}

/** The glyph's word, read after the row's name: "Settled", "Changed since you settled it". */
function GlyphWord({ glyph }: { glyph: RowGlyph | null }) {
  if (!glyph?.word) return null;
  return (
    <>
      {" "}
      <span className="sr-only">{glyph.word}</span>
    </>
  );
}

/** The plan's pressure record, read aloud only: the plan page draws it (the spine draws no flask). */
function PressureWord({ pressure }: { pressure: PlanPressure }) {
  const { date } = pressure;
  // The date is the schema's string; `visibleControls` keeps the word readable as text.
  const word = date === null ? NOT_PRESSURE_TESTED : `${PRESSURE_TESTED} ${visibleControls(date)}`;
  return (
    <>
      {" "}
      <span className="sr-only">{word}</span>
    </>
  );
}

function ChildRow({ child, pathname }: { child: SpineChild; pathname: string }) {
  const body = (
    <>
      {child.glyph !== null && <GlyphMark glyph={child.glyph} />}
      {/* The spaces keep the spans apart in the link's accessible name ("01 Name", "slug Done"). */}
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
      <GlyphWord glyph={child.glyph} />
      {child.note !== null && (
        <>
          {" "}
          <span className="plan-status plan-fine">
            <Untrusted text={child.note} />
          </span>
        </>
      )}
      {child.pressure !== null && <PressureWord pressure={child.pressure} />}
    </>
  );
  const title = child.title ?? undefined;
  if (child.to === null) {
    return (
      <li className="plan-spine__child" title={title}>
        {body}
      </li>
    );
  }
  return (
    <li>
      <Link
        to={child.to}
        title={title}
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
  const own = group.glyph;
  return (
    <Link
      to={group.to}
      aria-current={selected ? "page" : undefined}
      className="plan-spine__link plan-spine__head"
    >
      {/* The vision and metaplan lead with their own glyph; the other groups with the step's mark. */}
      {own ? <GlyphMark glyph={own} /> : <Mark className="plan-spine__mark" aria-hidden="true" />}
      <span className="plan-spine__name">{group.label}</span>
      <GlyphWord glyph={own} />{" "}
      {group.count !== null ? (
        <>
          <span className="plan-fine">
            <Untrusted text={group.unit === null ? group.count : `${group.count} ${group.unit}`} />
          </span>
          {/* A shown unit ("6 done") already says it; the word is read only beside a bare count. */}
          {group.unit === null && <span className="sr-only">, {group.word}</span>}
        </>
      ) : (
        // The glyph's word already says "Settled" or "Draft"; the step's word stays where it adds.
        !own?.word && <span className="plan-fine">{group.word}</span>
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
