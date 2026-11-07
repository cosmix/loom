import { ChevronRight, Files } from "lucide-react";
import { useId, useState, type JSX } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/ui/plural";

import { BRIEFS_FOLDED_OVER, briefGroups, type BriefGroup } from "./brief-groups";
import { bundleLinks, type BundleLink } from "./model";
import { useTrace } from "./state";

import "./briefs.css";

/**
 * One folder's briefs: the folder in mono over its files as chips. A chip shows the file's stem;
 * its link still reads the whole path under the bundle (`chat-server/c1-wire.md`), so the folder
 * is not lost to a screen reader that lands on the chip.
 */
function Group({ group }: { group: BriefGroup }): JSX.Element {
  const prefix = group.folder === "" ? "" : `${group.folder}/`;
  return (
    <li className="trace-briefs__group">
      {group.folder !== "" && (
        <p className="trace-briefs__folder">
          <Untrusted text={group.folder} />
          <span className="trace-briefs__tally">{group.briefs.length}</span>
        </p>
      )}
      <ul className="trace-briefs__files">
        {group.briefs.map((brief) => (
          <li key={brief.path}>
            <Link to={brief.to} className="trace-brief" title={`${prefix}${brief.name}`}>
              {prefix && <span className="sr-only">{prefix}</span>}
              <Untrusted text={brief.stem} />
              {brief.stem !== brief.name && <span className="sr-only">.md</span>}
            </Link>
          </li>
        ))}
      </ul>
    </li>
  );
}

/** The folders behind a folded bundle, so its head says what it holds: "chat-server 8 · common". */
function folderSummary(groups: BriefGroup[]): string {
  return groups
    .map((group) =>
      group.folder === ""
        ? group.briefs.map((brief) => brief.stem).join(" · ")
        : `${group.folder} ${group.briefs.length}`,
    )
    .join(" · ");
}

function BriefsBody({ links }: { links: BundleLink[] }): JSX.Element {
  const groups = briefGroups(links);
  const foldable = links.length > BRIEFS_FOLDED_OVER;
  const [open, setOpen] = useState(!foldable);
  const titleId = useId();
  const listId = useId();
  const count = plural(links.length, "brief");
  return (
    <section className="trace-briefs plan-surface" aria-labelledby={titleId}>
      <div className="trace-briefs__head">
        <Files aria-hidden="true" className="trace-briefs__glyph" />
        <p id={titleId} className="trace-briefs__title">
          Briefs
        </p>
        {foldable ? (
          <Button
            type="button"
            size="sm"
            data-look="quiet"
            className="plan-button trace-briefs__toggle"
            aria-expanded={open}
            aria-controls={listId}
            onClick={() => setOpen(!open)}
          >
            {count}
            <ChevronRight aria-hidden="true" />
          </Button>
        ) : (
          <span className="trace-briefs__count">{count}</span>
        )}
      </div>
      {!open && (
        <p className="trace-briefs__summary">
          <Untrusted text={folderSummary(groups)} />
        </p>
      )}
      <ul id={listId} aria-label="Briefs" className="trace-briefs__groups" hidden={!open}>
        {groups.map((group) => (
          <Group key={group.folder} group={group} />
        ))}
      </ul>
    </section>
  );
}

/**
 * A plan's brief files in a card of their own under its stub: a head with the count, then the
 * files grouped by folder as chips linked to their source pages. A bundle over six files starts
 * folded to its head and a one-line summary of its folders. Nothing renders for a document with
 * no briefs.
 */
export function BriefsCard(props: { feature: string; path: string }): JSX.Element | null {
  const { trace } = useTrace(props.feature);
  const links = trace ? bundleLinks(trace, props.path, props.feature) : [];
  if (links.length === 0) return null;
  // Keyed by plan: another plan's bundle opens folded or not by its own size.
  return <BriefsBody key={props.path} links={links} />;
}
