// Radix returns focus to a dialog's trigger when it closes. A dialog whose opener is gone by then
// (the control unmounted, or opened from a floating control) would drop focus to the page body;
// these pick the stable place to return it instead.

/** The document region the reader works in: the file view's scroll region, else the page main. */
function readingRegion(): HTMLElement | null {
  // Two queries, not one `"[data-reading-scroll], main"`: the page `<main>` contains the file
  // view's scroll region and precedes it in document order, so one selector would always find
  // `<main>`, which does not scroll there.
  return (
    document.querySelector<HTMLElement>("[data-reading-scroll]") ??
    document.querySelector<HTMLElement>("main")
  );
}

/**
 * Focus the reading region, making a region that is not focusable yet programmatically so. That
 * tabindex lasts only while the region holds focus: left behind, a click anywhere in the region
 * would focus it and draw a focus ring around the whole page.
 */
export function focusReadingRegion(): void {
  const region = readingRegion();
  if (!region) return;
  if (!region.hasAttribute("tabindex")) {
    region.tabIndex = -1;
    region.addEventListener("blur", () => region.removeAttribute("tabindex"), { once: true });
  }
  region.focus({ preventScroll: true });
}

/**
 * The nearest focusable ancestor of the live selection (a passage or the reading column), else
 * the reading region. Read it before the dialog opens: focus entering the dialog may clear the
 * selection.
 */
export function selectionFocusTarget(): HTMLElement | null {
  const selection = window.getSelection();
  const range = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
  const node = range?.commonAncestorContainer;
  const element = node instanceof Element ? node : node?.parentElement;
  return element?.closest<HTMLElement>("[tabindex]") ?? readingRegion();
}

/** Focus `target` if it is still on the page, else the reading region. */
export function returnFocusTo(target: HTMLElement | null): void {
  if (target?.isConnected) target.focus();
  else focusReadingRegion();
}
