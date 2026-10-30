import { Link, NavLink, useLocation } from "react-router";

import { cn } from "cn";
import { isPlanTabActive, PLAN_TABS, type PlanTab } from "@/plan/tabs";
import { VIEW_GLYPHS, type ViewId } from "@/plan/ui/glyphs";

const HomeIcon = VIEW_GLYPHS.home;

function isViewId(id: string): id is ViewId {
  return Object.hasOwn(VIEW_GLYPHS, id);
}

function ViewTab({ tab, pathname }: { tab: PlanTab; pathname: string }) {
  const Icon = isViewId(tab.id) ? VIEW_GLYPHS[tab.id] : undefined;
  const active = isPlanTabActive(tab, pathname);
  return (
    <Link
      to={tab.path}
      className={cn("view-tab", active && "is-active")}
      aria-current={active ? "page" : undefined}
    >
      {Icon && <Icon className="size-3.5" aria-hidden="true" />}
      <span className="max-sm:sr-only">{tab.label}</span>
    </Link>
  );
}

/**
 * Home and the registered views, as quiet links. `view-switch` keeps the dashboard's touch
 * sizing; `plan-view-switch` (header.css) restyles it for the planner.
 */
export function Tabs({ tabs = PLAN_TABS }: { tabs?: PlanTab[] }) {
  const { pathname } = useLocation();
  return (
    <nav aria-label="Views" className="view-switch plan-view-switch">
      <NavLink to="/" end className={({ isActive }) => cn("view-tab", isActive && "is-active")}>
        <HomeIcon className="size-3.5" aria-hidden="true" />
        <span className="max-sm:sr-only">Home</span>
      </NavLink>
      {tabs.map((tab) => (
        <ViewTab key={tab.id} tab={tab} pathname={pathname} />
      ))}
    </nav>
  );
}
