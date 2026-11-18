import { useEffect, useMemo, useState, type JSX } from "react";
import { Link } from "react-router";

import { cn } from "cn";
import type { SectionView } from "@/plan/api/schema-files";
import { SentenceText } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";

type Group = { head: SectionView; children: SectionView[] };

/** `##` headings with the `###` headings that follow each; other levels are not listed. */
function nest(sections: SectionView[]): Group[] {
  const groups: Group[] = [];
  for (const section of sections) {
    if (section.level === 2) groups.push({ head: section, children: [] });
    else if (section.level === 3 && groups.length > 0) groups.at(-1)?.children.push(section);
  }
  return groups;
}

/** The anchor of the first listed heading currently in view; kept while none is. */
function useHeadingInView(anchors: string[]): string | null {
  const [active, setActive] = useState<string | null>(null);
  const key = anchors.join("\n");
  useEffect(() => {
    if (typeof IntersectionObserver === "undefined") return;
    const visible = new Set<string>();
    const order = key ? key.split("\n") : [];
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) visible.add(entry.target.id);
        else visible.delete(entry.target.id);
      }
      const first = order.find((anchor) => visible.has(anchor));
      if (first) setActive(first);
    });
    for (const anchor of order) {
      const element = document.getElementById(anchor);
      if (element) observer.observe(element);
    }
    return () => observer.disconnect();
  }, [key]);
  return active;
}

function OutlineLink({
  path,
  section,
  active,
}: {
  path: string;
  section: SectionView;
  active: boolean;
}): JSX.Element {
  return (
    <Link
      to={toAddress(path, section.anchor)}
      aria-current={active ? "location" : undefined}
      className={cn(
        "block truncate py-0.5 text-xs hover:text-foreground",
        active ? "font-medium text-foreground" : "text-muted-foreground",
      )}
    >
      <span style={{ unicodeBidi: "isolate", direction: "ltr" }}>
        <SentenceText text={section.title} />
      </span>
    </Link>
  );
}

/** Headings of the open document, nested under its tree entry. */
export function Outline({
  path,
  sections,
}: {
  path: string;
  sections: SectionView[];
}): JSX.Element | null {
  const groups = useMemo(() => nest(sections), [sections]);
  const active = useHeadingInView(
    useMemo(() => groups.flatMap((g) => [g.head, ...g.children]).map((s) => s.anchor), [groups]),
  );
  if (groups.length === 0) return null;
  return (
    <ul aria-label="Outline" className="ml-5 border-l border-hairline pl-2">
      {groups.map(({ head, children }) => (
        <li key={head.anchor}>
          <OutlineLink path={path} section={head} active={active === head.anchor} />
          {children.length > 0 && (
            <ul className="ml-3">
              {children.map((child) => (
                <li key={child.anchor}>
                  <OutlineLink path={path} section={child} active={active === child.anchor} />
                </li>
              ))}
            </ul>
          )}
        </li>
      ))}
    </ul>
  );
}
