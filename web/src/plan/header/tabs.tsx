import { NavLink } from "react-router";

import { cn } from "cn";
import { PLAN_TABS, type PlanTab } from "@/plan/tabs";

/** Tabs registered in `PLAN_TABS`; renders nothing while none are. */
export function Tabs({ tabs = PLAN_TABS }: { tabs?: PlanTab[] }) {
  if (tabs.length === 0) return null;
  return (
    // `min-w-0` lets the row shrink below its content and `overflow-x-auto` scrolls the rest, so
    // the controls on the right (Chat, More) are never pushed out of a narrow header.
    <nav
      aria-label="Planning tabs"
      className="flex min-w-0 items-center gap-1 overflow-x-auto [scrollbar-width:none]"
    >
      {tabs.map((tab) => (
        <NavLink
          key={tab.id}
          to={tab.path}
          className={({ isActive }) =>
            cn(
              "shrink-0 whitespace-nowrap rounded-md px-2 py-1 text-[13px] font-medium",
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
