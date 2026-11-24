import { type JSX, type ReactNode, useId } from "react";

import { visibleBidi } from "@/lib/visible-bidi";
import type { RenderContext } from "@/plan/markdown/context";
import {
  type CoverGroup,
  type CoverRef,
  type CoversSource,
  groupCovers,
  parseCovers,
} from "@/plan/markdown/covers";
import { DocLink } from "@/plan/markdown/inline";

import "@/plan/markdown/covers.css";

function sectionNumber(number: number | null): string {
  return number === null ? "–" : String(number).padStart(2, "0");
}

/**
 * One item of a wrapping list. The dot after it is glued to its last word by a no-break space,
 * so a line never starts with a dot; the plain space after the dot is where the list wraps.
 */
function Item(props: { last: boolean; state?: string; children: ReactNode }): JSX.Element {
  return (
    <li data-state={props.state}>
      {props.children}
      {!props.last && (
        <span className="md-covers__sep" aria-hidden="true">
          {" ·"}
        </span>
      )}{" "}
    </li>
  );
}

/** A heading that resolved is a link to it; any other ref is its raw words, muted, never a link. */
function RefBody({ item, context }: { item: CoverRef; context: RenderContext }): JSX.Element {
  if (item.kind === "heading") {
    return (
      <DocLink path={item.path} anchor={item.anchor} context={context}>
        {visibleBidi(item.title)}
      </DocLink>
    );
  }
  const title =
    item.kind === "missing" ? `${visibleBidi(item.raw)}: not found in the spec` : undefined;
  return (
    <span className="md-covers__raw" title={title}>
      {visibleBidi(item.text)}
    </span>
  );
}

function SectionHead({ group, context }: { group: CoverGroup; context: RenderContext }) {
  const { section, number } = group;
  const unknown =
    number === null ? "Not a spec section" : `No section ${sectionNumber(number)} in the spec`;
  return (
    <dt>
      <span className="md-covers__number">{sectionNumber(number)}</span>{" "}
      {section ? (
        <DocLink path={section.path} anchor={null} context={context}>
          {visibleBidi(section.title)}
        </DocLink>
      ) : (
        <span className="md-covers__unknown">{unknown}</span>
      )}
    </dt>
  );
}

function GroupView({ group, context }: { group: CoverGroup; context: RenderContext }) {
  const count = group.refs.length;
  return (
    <div className="md-covers__group">
      <SectionHead group={group} context={context} />{" "}
      <dd>
        <ul className="md-covers__refs">
          {group.whole && (
            <Item last={count === 0} state="whole">
              Whole section
            </Item>
          )}
          {group.refs.map((item, index) => (
            <Item key={index} last={index === count - 1} state={item.kind}>
              <RefBody item={item} context={context} />
            </Item>
          ))}
        </ul>
      </dd>
    </div>
  );
}

/**
 * A metaplan entry's `Covers:` line drawn as the spec headings it names: one row per section,
 * its mono number and linked title, then its headings as a wrapping list of linked titles.
 */
export function CoversBlock(props: {
  value: string;
  covers: CoversSource;
  context: RenderContext;
}): JSX.Element {
  const labelId = useId();
  const groups = groupCovers(parseCovers(props.value), props.covers.lookup);
  return (
    <div className="md-covers" role="group" aria-labelledby={labelId}>
      <p className="md-covers__label" id={labelId}>
        Covers
      </p>{" "}
      <dl className="md-covers__list">
        {groups.map((group) => (
          <GroupView key={group.number ?? "other"} group={group} context={props.context} />
        ))}
      </dl>
    </div>
  );
}
