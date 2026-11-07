import { FileText } from "lucide-react";
import { useId, type JSX } from "react";
import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";
import { plural } from "@/plan/ui/plural";

import { briefGroups, type BriefGroup } from "./brief-groups";
import { bundleLinks } from "./model";
import { useTrace } from "./state";

import "./briefs.css";

/** One stage's briefs: its folder as the group's heading, then each file by its mono name. */
function Group({ group }: { group: BriefGroup }): JSX.Element {
  return (
    <li className="trace-briefs__group">
      {group.folder !== "" && (
        <h3 className="trace-briefs__folder">
          <Untrusted text={group.folder} />
        </h3>
      )}
      <ul className="trace-briefs__files">
        {group.briefs.map((brief) => (
          <li key={brief.path}>
            <Link to={brief.to} className="trace-brief">
              <FileText aria-hidden="true" />
              <code className="trace-brief__name">
                <Untrusted text={brief.name} />
              </code>
            </Link>
          </li>
        ))}
      </ul>
    </li>
  );
}

/** "12 files in 4 stages": the folders under the bundle are its stages. */
function countText(files: number, groups: BriefGroup[]): string {
  const stages = groups.filter((group) => group.folder !== "").length;
  return stages === 0
    ? plural(files, "file")
    : `${plural(files, "file")} in ${plural(stages, "stage")}`;
}

/**
 * A plan's brief files on its summary page: every file of the bundle, grouped by the folder (the
 * stage) it sits in, each its mono name linked to its source page. The trace carries no brief
 * text, so there are no descriptions. Nothing renders for a plan with no briefs.
 */
export function BriefsCard(props: { feature: string; path: string }): JSX.Element | null {
  const { trace } = useTrace(props.feature);
  const titleId = useId();
  const links = trace ? bundleLinks(trace, props.path, props.feature) : [];
  if (links.length === 0) return null;
  const groups = briefGroups(links);
  return (
    <section className="trace-briefs" aria-labelledby={titleId}>
      <h2 id={titleId} className="trace-plan__heading">
        Briefs
      </h2>
      <p className="trace-plan__count">{countText(links.length, groups)}</p>
      <ul aria-label="Briefs" className="trace-briefs__groups">
        {groups.map((group) => (
          <Group key={group.folder} group={group} />
        ))}
      </ul>
    </section>
  );
}
