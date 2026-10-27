import { NavLink } from "react-router";

import { cn } from "cn";
import { PLAN_TABS, type PlanTab } from "@/plan/tabs";

/** Tabs registered in `PLAN_TABS`; renders nothing while none are. */
export function Tabs({ tabs = PLAN_TABS }: { tabs?: PlanTab[] }) {
  if (tabs.length === 0) return null;
  return (
    <nav aria-label="Planning tabs" className="flex items-center gap-1">
      {tabs.map((tab) => (
        <NavLink
          key={tab.id}
          to={tab.path}
          className={({ isActive }) =>
            cn(
              "rounded-md px-2 py-1 text-[13px] font-medium",
              isActive
                ? "bg-(--highlight) text-(--highlight-foreground)"
                : "text-muted-foreground hover:text-foreground",
            )
          }
        >
          {tab.label}
        </NavLink>
      ))}
    </nav>
  );
}
