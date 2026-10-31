import { useAtomValue, useStore } from "jotai";
import { useEffect, useLayoutEffect, useMemo, type JSX } from "react";
import { useInRouterContext, useLocation } from "react-router";

import { checklistAtom, standaloneAtom } from "@/plan/state/atoms";
import { bindAttachedSession } from "./context-attach";
import { offerErrorAtom } from "./offer-error";
import { screenAtom, screenOfPath, tabFeatureAtom } from "./session-home";
import { contextDismissedAtom, viewedDocumentAtom, viewedOfPath } from "./viewed-document";

function RoutedScreen(): null {
  const store = useStore();
  const { pathname } = useLocation();
  const checklist = useAtomValue(checklistAtom);
  const standalone = useAtomValue(standaloneAtom);
  const { feature, plan } = screenOfPath(pathname);
  const routeFeature = pathname.startsWith("/f/") ? feature : null;
  const viewed = useMemo(
    () => viewedOfPath(pathname, checklist, standalone),
    [pathname, checklist, standalone],
  );
  // Before paint, so a navigation never shows one frame of the previous screen's session.
  useLayoutEffect(() => {
    store.set(screenAtom, { feature, plan });
    store.set(tabFeatureAtom, routeFeature);
    // A refusal is about the screen it was raised on.
    store.set(offerErrorAtom, null);
  }, [store, feature, plan, routeFeature]);
  // The context line and its placeholder follow the route in the same pass, with nothing clicked.
  useLayoutEffect(() => {
    store.set(viewedDocumentAtom, (old) =>
      old.kind === viewed.kind && old.path === viewed.path && old.name === viewed.name
        ? old
        : viewed,
    );
  }, [store, viewed]);
  // A dismissal follows the document: it survives a route change only to the same file (the
  // vision and the product overview share one; so do /f/ and /files/ addresses) and ends at any
  // other page, including a page without a source. It runs after the effect above has written
  // the new page, and only on a pathname change: a checklist arriving on the same page never
  // brings the line back.
  useLayoutEffect(() => {
    const { path } = store.get(viewedDocumentAtom);
    store.set(contextDismissedAtom, (dismissed) =>
      dismissed !== null && path !== null && dismissed.path === path ? dismissed : null,
    );
  }, [store, pathname]);
  useEffect(() => bindAttachedSession(store), [store]);
  return null;
}

/** Keeps `screenAtom` and the viewed document on the current route; renders nothing, and does nothing outside a router. */
export function ScreenSync(): JSX.Element | null {
  return useInRouterContext() ? <RoutedScreen /> : null;
}
