import { CircleAlert, Inbox, SearchX, type LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { Spinner } from "@/plan/ui/spinner";

import "./load-state.css";

type State = "loading" | "empty" | "not_found" | "error";

interface LoadStateProps {
  state: State;
  title?: string;
  message?: string;
  action?: { label: string; onClick: () => void };
  /** More of the action row: a link out, or a second way on. */
  children?: ReactNode;
  /** `compact` tightens the padding for a popover, a card, a dialog section or a sidebar. */
  size?: "page" | "compact";
}

const GLYPHS: Record<Exclude<State, "loading">, LucideIcon> = {
  empty: Inbox,
  not_found: SearchX,
  error: CircleAlert,
};

const TITLES: Record<State, string> = {
  loading: "Loading",
  empty: "Nothing here yet",
  not_found: "Not found",
  error: "Something went wrong",
};

/**
 * The planner's one loading, empty and error state (`ui/DESIGN.md`): a glyph, a title, an
 * optional detail and an optional action row, centred in whatever holds it. It fills a flex
 * column's spare height and, as an only child, its parent's height, so the container decides where
 * the middle is. Loading draws the spinner under `role="status"` named by the title; the roundel
 * means activity and is never drawn here. `message` is often a server's or an error's text, so it
 * is drawn through Untrusted.
 */
export function LoadState({
  state,
  title,
  message,
  action,
  children,
  size = "page",
}: LoadStateProps) {
  const label = title ?? TITLES[state];
  const loading = state === "loading";
  const Glyph = loading ? null : GLYPHS[state];
  const actions = (action || children) && (
    <div className="plan-state__actions">
      {action && (
        <Button data-look="secondary" className="plan-button" onClick={action.onClick}>
          {action.label}
        </Button>
      )}
      {children}
    </div>
  );
  return (
    <div
      className="plan-state"
      data-state={state}
      data-size={size}
      role={loading ? "status" : undefined}
      aria-label={loading ? label : undefined}
    >
      <span className="plan-state__glyph">
        {Glyph ? <Glyph size={20} strokeWidth={1.5} aria-hidden="true" /> : <Spinner />}
      </span>
      <p className="plan-state__title">{label}</p>
      {message && (
        <p className="plan-state__detail">
          <Untrusted text={message} />
        </p>
      )}
      {actions}
    </div>
  );
}
