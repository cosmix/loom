import { cn } from "cn";
import { Rows3Icon, WorkflowIcon } from "lucide-react";
import { NavLink } from "react-router";

const VIEWS = [
  { to: "/", label: "graph", Icon: WorkflowIcon, end: true },
  { to: "/ledger", label: "ledger", Icon: Rows3Icon, end: false },
] as const;

/// Graph or ledger, as a segmented control in the header.
export function ViewSwitch() {
  return (
    <nav aria-label="view" className="view-switch">
      {VIEWS.map(({ to, label, Icon, end }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          className={({ isActive }) => cn("view-tab", isActive && "is-active")}
        >
          <Icon className="size-3.5" aria-hidden="true" />
          <span className="hidden sm:inline">{label}</span>
          <span className="sr-only sm:hidden">{label}</span>
        </NavLink>
      ))}
    </nav>
  );
}
