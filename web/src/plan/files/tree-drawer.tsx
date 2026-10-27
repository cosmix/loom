import { MenuIcon } from "lucide-react";
import { useCallback, useEffect, useRef, useState, type JSX } from "react";
import { useLocation } from "react-router";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "@/components/ui/dialog";

/**
 * The document list as a modal drawer below the wide breakpoint. Radix's dialog supplies
 * what a modal needs: focus moves in on open and stays in, the page behind is hidden from
 * assistive technology, and the opener (which carries aria-expanded and aria-controls)
 * gets focus back on close.
 */
export function useTreeDrawer() {
  const { pathname, search, hash, key } = useLocation();
  const [open, setOpen] = useState(false);
  const navigated = useRef(false);
  const reading = useRef<HTMLDivElement>(null);

  // A chosen document, a search hit or an outline entry moves the location (the anchor lives
  // in the hash): the drawer has done its job and must not keep covering the content. Tapping
  // the open document's own entry keeps the URL and renews only `key`.
  useEffect(() => {
    navigated.current = true;
    setOpen(false);
  }, [pathname, search, hash, key]);

  // Radix reports the opener's own toggles and dismissals, never the close above.
  const onOpenChange = useCallback((next: boolean) => {
    navigated.current = false;
    setOpen(next);
  }, []);

  // After choosing something the reader wants the content, not the button that opened the list.
  const onCloseFocus = useCallback((event: Event) => {
    if (!navigated.current) return;
    event.preventDefault();
    reading.current?.focus();
  }, []);

  return { open, onOpenChange, onCloseFocus, reading };
}

export type TreeDrawerState = ReturnType<typeof useTreeDrawer>;

export function TreeDrawerRoot({
  drawer,
  children,
}: {
  drawer: TreeDrawerState;
  children: JSX.Element;
}): JSX.Element {
  return (
    <Dialog open={drawer.open} onOpenChange={drawer.onOpenChange}>
      {children}
    </Dialog>
  );
}

/** The bar with the control that opens the drawer; render it inside `TreeDrawerRoot`. */
export function DrawerOpener(): JSX.Element {
  return (
    <div className="border-b border-hairline px-4 py-2">
      <DialogTrigger asChild>
        <Button variant="ghost" size="sm">
          <MenuIcon aria-hidden="true" />
          Documents
        </Button>
      </DialogTrigger>
    </div>
  );
}

/** The drawer itself, a sheet from the left edge; render it inside `TreeDrawerRoot`. */
export function TreeDrawer({
  drawer,
  children,
}: {
  drawer: TreeDrawerState;
  children: JSX.Element;
}): JSX.Element {
  const content = useRef<HTMLDivElement>(null);
  const { onOpenChange } = drawer;
  // The viewport became wide: the tree is a column now, and the drawer must not come back
  // open the next time the screen narrows.
  useEffect(() => () => onOpenChange(false), [onOpenChange]);

  return (
    <DialogContent
      ref={content}
      aria-modal="true"
      aria-describedby={undefined}
      showCloseButton={false}
      className="top-0 left-0 flex h-full w-72 max-w-[85vw] translate-x-0 translate-y-0 flex-col gap-0 rounded-none border-r border-hairline bg-background p-0 ring-0 sm:max-w-[85vw]"
      // Radix would focus the search box, which raises the phone keyboard over the list.
      // Focus still lands inside the dialog, on the sheet.
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        content.current?.focus();
      }}
      onCloseAutoFocus={drawer.onCloseFocus}
      // Escape is handled here, after the search box has had its say: an Esc that cleared the
      // box stops there (see SearchBox) and leaves the drawer open.
      onEscapeKeyDown={(event) => event.preventDefault()}
      onKeyDown={(event) => {
        if (event.key === "Escape") drawer.onOpenChange(false);
      }}
    >
      <DialogTitle className="sr-only">Documents</DialogTitle>
      {children}
    </DialogContent>
  );
}
