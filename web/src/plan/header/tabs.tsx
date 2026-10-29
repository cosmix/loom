import { NavLink } from "react-router";

import { cn } from "cn";
import { PLAN_TABS, type PlanTab } from "@/plan/tabs";
import { VIEW_GLYPHS, type ViewId } from "@/plan/ui/glyphs";

const HomeIcon = VIEW_GLYPHS.home;

function isViewId(id: string): id is ViewId {
  return Object.hasOwn(VIEW_GLYPHS, id);
}

/** Home and registered planning views, as a segmented control in the header. */
export function Tabs({ tabs = PLAN_TABS }: { tabs?: PlanTab[] }) {
  return (
    <nav aria-label="Views" className="view-switch">
      <NavLink to="/" end className={({ isActive }) => cn("view-tab", isActive && "is-active")}>
        <HomeIcon className="size-3.5" aria-hidden="true" />
        <span className="max-sm:sr-only">Home</span>
      </NavLink>
      {tabs.map((tab) => {
        const Icon = isViewId(tab.id) ? VIEW_GLYPHS[tab.id] : undefined;
        return (
          <NavLink
            key={tab.id}
            to={tab.path}
            className={({ isActive }) => cn("view-tab", isActive && "is-active")}
          >
            {Icon && <Icon className="size-3.5" aria-hidden="true" />}
            <span className="max-sm:sr-only">{tab.label}</span>
          </NavLink>
        );
      })}
    </nav>
  );
}
