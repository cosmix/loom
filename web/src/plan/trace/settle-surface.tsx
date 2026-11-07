import { useAtomValue } from "jotai/react";
import { Dialog } from "radix-ui";
import type { JSX, ReactNode, RefObject } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import { PopoverContent } from "@/plan/header/popover";
import { readingFaceAtom, readingSizeAtom } from "@/plan/settings/preferences";

/** Below this width the settle menus and the confirmation rise as bottom sheets (design.md). */
const NARROW = "(max-width: 759.98px)";

export function useNarrow(): boolean {
  return useMediaQuery(NARROW);
}

/**
 * A portal leaves the shell's `[data-loom-plan]` root, which carries the reading face and the
 * planner's stylesheet scope, so the surface carries the root's attributes itself.
 */
function useRootAttributes() {
  const face = useAtomValue(readingFaceAtom);
  const size = useAtomValue(readingSizeAtom);
  return {
    "data-loom-plan": "",
    "data-reading-face": face,
    "data-reading-size": String(size),
  };
}

interface SurfaceProps {
  label: string;
  showTitle: boolean;
  closeFocus: (event: Event) => void;
  children: ReactNode;
}

function PopoverSurface(props: SurfaceProps & { button: RefObject<HTMLElement | null> }) {
  const { label, showTitle, closeFocus, button, children } = props;
  const attributes = useRootAttributes();
  return (
    <PopoverContent
      {...attributes}
      aria-label={label}
      align="start"
      className="trace-surface w-[min(28rem,calc(100vw-2rem))]"
      onCloseAutoFocus={closeFocus}
      onInteractOutside={(event) => {
        if (event.target instanceof Node && button.current?.contains(event.target)) {
          event.preventDefault();
        }
      }}
    >
      {showTitle && <h2 className="trace-surface__title">{label}</h2>}
      {children}
    </PopoverContent>
  );
}

function SheetSurface(props: SurfaceProps & { open: boolean; onClose: () => void }) {
  const { open, onClose, label, showTitle, closeFocus, children } = props;
  const attributes = useRootAttributes();
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-50 bg-(--dialog-backdrop)" />
        <Dialog.Content
          {...attributes}
          data-sheet=""
          className="trace-surface trace-sheet fixed inset-x-0 bottom-0 z-50 outline-none"
          aria-describedby={undefined}
          onCloseAutoFocus={closeFocus}
        >
          <Dialog.Title className={showTitle ? "trace-surface__title" : "sr-only"}>
            {label}
          </Dialog.Title>
          {children}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

/**
 * What the settle control opens: a popover under its button, or a bottom sheet (`data-sheet`) on a
 * narrow screen. Both hold the same content and take focus back to the button when they close.
 * `label` names the surface; `showTitle` draws it as a heading (the confirmation) and otherwise
 * keeps it for assistive technology only (a menu). The caller draws the popover's `Popover` root
 * and anchor; the sheet needs neither.
 */
export function SettleSurface(props: {
  open: boolean;
  narrow: boolean;
  label: string;
  showTitle: boolean;
  onClose: () => void;
  returnFocus: () => void;
  /** The button that opened it: a press on it toggles the popover, so it is not "outside". */
  button: RefObject<HTMLElement | null>;
  children: ReactNode;
}): JSX.Element {
  const { open, narrow, label, showTitle, onClose, returnFocus, button, children } = props;
  const closeFocus = (event: Event) => {
    // No Radix trigger opened the surface, so the default would drop focus on the body.
    event.preventDefault();
    returnFocus();
  };
  const shared = { label, showTitle, closeFocus, children };
  return narrow ? (
    <SheetSurface {...shared} open={open} onClose={onClose} />
  ) : (
    <PopoverSurface {...shared} button={button} />
  );
}
