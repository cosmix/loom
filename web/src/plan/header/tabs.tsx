import { Link, useLocation } from "react-router";

import { cn } from "cn";
import { isPlanTabActive, PLAN_TABS, type PlanTab } from "@/plan/tabs";
import { VIEW_GLYPHS, type ViewId } from "@/plan/ui/glyphs";

function isViewId(id: string): id is ViewId {
  return Object.hasOwn(VIEW_GLYPHS, id);
}

function ViewTab({
  tab,
  pathname,
  onSelect,
}: {
  tab: PlanTab;
  pathname: string;
  onSelect?: () => void;
}) {
  const Icon = isViewId(tab.id) ? VIEW_GLYPHS[tab.id] : undefined;
  const active = isPlanTabActive(tab, pathname);
  return (
    <Link
      to={tab.path}
      onClick={onSelect}
      className={cn("view-tab", active && "is-active")}
      aria-current={active ? "page" : undefined}
    >
      {Icon && <Icon className="size-3.5" aria-hidden="true" />}
      <span>{tab.label}</span>
    </Link>
  );
}

export function Tabs({ tabs = PLAN_TABS, onSelect }: { tabs?: PlanTab[]; onSelect?: () => void }) {
  const { pathname } = useLocation();
  if (!tabs.length) return null;
  return (
    <nav aria-label="Views" className="view-switch plan-view-switch">
      {tabs.map((tab) => (
        <ViewTab key={tab.id} tab={tab} pathname={pathname} onSelect={onSelect} />
      ))}
    </nav>
  );
}
