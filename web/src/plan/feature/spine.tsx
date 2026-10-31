import { useAtom } from "jotai/react";
import { ChevronDown } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useLocation } from "react-router";

import { z } from "@/api/zod";
import { Button } from "@/components/ui/button";
import { projectPref } from "@/plan/chat/prefs";
import { NextAction } from "@/plan/feature/next-action";
import { GroupRow, Name } from "@/plan/feature/spine-row";
import {
  groupOfPathname,
  spineGroups,
  type GroupId,
  type NameStyle,
  type SpineGroup,
} from "@/plan/feature/spine-model";
import { useFeature } from "@/plan/feature/use-feature";

import "./spine.css";

/** The groups the reader has expanded, per feature. The group the route is in is always open. */
const expandedAtom = projectPref(
  "spine-expanded",
  {} as Record<string, string[]>,
  z.record(z.string(), z.array(z.string())),
);

/** The row the route is on, named for the "Documents" control a narrow screen collapses to. */
function currentRow(
  groups: SpineGroup[],
  pathname: string,
): { name: string; style: NameStyle } | null {
  for (const group of groups) {
    const child = group.children.find((candidate) => candidate.to === pathname);
    if (child) return { name: child.name, style: child.style };
    if (pathname === group.to || groupOfPathname(pathname) === group.id) {
      return { name: group.label, style: "plain" };
    }
  }
  return null;
}

/** The reader's expanded groups for one feature, and the toggle that changes them. */
function useExpandedGroups(feature: string): { opened: string[]; toggle: (id: GroupId) => void } {
  const [expanded, setExpanded] = useAtom(expandedAtom);
  // A feature is a server-chosen slug: `constructor` must not read Object.prototype.
  const opened = (Object.hasOwn(expanded, feature) ? expanded[feature] : undefined) ?? [];
  const toggle = (id: GroupId) =>
    setExpanded({
      ...expanded,
      [feature]: opened.includes(id)
        ? opened.filter((candidate) => candidate !== id)
        : [...opened, id],
    });
  return { opened, toggle };
}

/** The control a narrow screen shows in place of the list: "Documents" and the row it is on. */
function MenuButton({
  open,
  controls,
  row,
  onClick,
}: {
  open: boolean;
  controls: string;
  row: { name: string; style: NameStyle } | null;
  onClick: () => void;
}) {
  return (
    <Button
      type="button"
      data-look="secondary"
      className="plan-button plan-spine__menu"
      aria-expanded={open}
      aria-controls={controls}
      onClick={onClick}
    >
      <span>Documents</span>
      {row && (
        <span className="plan-spine__here">
          <Name name={row.name} style={row.style} />
        </span>
      )}
      <ChevronDown aria-hidden="true" />
    </Button>
  );
}

/**
 * The feature's documents in workflow order, with the next step at its head: the only place the
 * screen draws a feature's progress. Below 760px the list sits behind a "Documents" control that
 * names the current row; the list stays in the document either way (CSS hides it), so it never
 * scrolls sideways.
 */
export function Spine() {
  const { checklist } = useFeature();
  const { pathname } = useLocation();
  const [menuOpen, setMenuOpen] = useState(false);
  const bodyId = useId();
  const { opened, toggle } = useExpandedGroups(checklist?.feature ?? "");
  useEffect(() => setMenuOpen(false), [pathname]);
  if (!checklist) return null;

  const groups = spineGroups(checklist);
  const here = groupOfPathname(pathname);
  return (
    <nav
      aria-label="Feature documents"
      className="plan-spine"
      data-open={menuOpen ? "" : undefined}
    >
      <MenuButton
        open={menuOpen}
        controls={bodyId}
        row={currentRow(groups, pathname)}
        onClick={() => setMenuOpen(!menuOpen)}
      />
      <div id={bodyId} className="plan-spine__body">
        <NextAction checklist={checklist} />
        <ol className="plan-spine__groups">
          {groups.map((group) => (
            <GroupRow
              key={group.id}
              group={group}
              pathname={pathname}
              here={group.id === here}
              open={group.id === here || opened.includes(group.id)}
              onToggle={() => toggle(group.id)}
            />
          ))}
        </ol>
      </div>
    </nav>
  );
}
