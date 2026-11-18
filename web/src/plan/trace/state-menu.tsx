import { cn } from "cn";
import { ChevronDown } from "lucide-react";
import { Popover as PopoverPrimitive } from "radix-ui";
import {
  useId,
  useRef,
  useState,
  type ComponentProps,
  type JSX,
  type ReactNode,
  type RefObject,
} from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Popover } from "@/plan/header/popover";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { HEADING_BUTTON } from "@/plan/ui/doc-heading";

import { SettleSurface, useNarrow } from "./settle-surface";

import "./trace.css";

// A state button and the menu it opens, shared by the settle control and a plan's Pressure-test:
// the button's label is the state, a chevron follows it, and a press opens a popover under it (a
// bottom sheet on a narrow screen) holding the choices, each a verb over a one-line note.

/**
 * The button: the caller's glyph and state words, then the chevron while it opens a menu. It is
 * the popover's anchor (no Radix trigger opens the surface), so it must stand inside a `Popover`.
 * `busy` is work the button started: no chevron then, its words say what runs.
 */
export function StateMenuButton(
  props: Omit<ComponentProps<"button">, "type" | "ref"> & {
    buttonRef: RefObject<HTMLButtonElement | null>;
    menu: boolean;
    open: boolean;
    busy?: boolean;
  },
): JSX.Element {
  const { buttonRef, menu, open, busy = false, className, children, ...rest } = props;
  return (
    <PopoverPrimitive.Anchor asChild>
      <button
        ref={buttonRef}
        type="button"
        {...HEADING_BUTTON}
        className={cn(HEADING_BUTTON.className, className)}
        aria-busy={busy || undefined}
        aria-haspopup={menu ? "dialog" : undefined}
        aria-expanded={menu ? open : undefined}
        {...rest}
      >
        {children}
        {menu && !busy && <ChevronDown className="trace-menu-chevron" aria-hidden="true" />}
      </button>
    </PopoverPrimitive.Anchor>
  );
}

export function StateMenuList({ children }: { children: ReactNode }): JSX.Element {
  return <ul className="trace-menu">{children}</ul>;
}

/**
 * One choice: the verb over its note. The note describes the button, so the button's own name
 * stays the verb. A blocked choice stays focusable and draws its reason in the note's place.
 */
export function StateMenuItem(props: {
  label: string;
  note: string;
  reason?: string | null;
  onChoose: () => void;
}): JSX.Element {
  const { label, note, reason = null, onChoose } = props;
  const noteId = useId();
  return (
    <li className="trace-menu__item">
      <DisabledReason
        look="quiet"
        size="sm"
        reason={reason}
        describedBy={reason === null ? noteId : undefined}
        onClick={onChoose}
      >
        {label}
      </DisabledReason>
      {reason === null && (
        <p id={noteId} className="trace-menu__note">
          {note}
        </p>
      )}
    </li>
  );
}

/** A choice that goes somewhere: the same verb over its note, as a link. */
export function StateMenuLink(props: {
  label: string;
  note: string;
  to: string;
  onChoose: () => void;
}): JSX.Element {
  const noteId = useId();
  return (
    <li className="trace-menu__item">
      <Button asChild size="sm" data-look="quiet" className="plan-button">
        <Link to={props.to} aria-describedby={noteId} onClick={props.onChoose}>
          {props.label}
        </Link>
      </Button>
      <p id={noteId} className="trace-menu__note">
        {props.note}
      </p>
    </li>
  );
}

/**
 * A state button with nothing but a menu behind it: `glyph` and `label` are the button's state,
 * `label` also names the surface, and `children` draws the choices given the menu's `close`.
 * Escape and a press outside close it, and focus returns to the button.
 */
export function StateMenu(props: {
  label: string;
  glyph: ReactNode;
  children: (close: () => void) => ReactNode;
}): JSX.Element {
  const { label, glyph, children } = props;
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const narrow = useNarrow();
  const close = () => setOpen(false);
  return (
    <Popover open={open && !narrow} onOpenChange={(next) => !next && close()}>
      <StateMenuButton buttonRef={button} menu open={open} onClick={() => setOpen((was) => !was)}>
        {glyph}
        {label}
      </StateMenuButton>
      <SettleSurface
        open={open}
        narrow={narrow}
        label={label}
        showTitle={false}
        onClose={close}
        returnFocus={() => button.current?.focus()}
        button={button}
      >
        <StateMenuList>{children(close)}</StateMenuList>
      </SettleSurface>
    </Popover>
  );
}
