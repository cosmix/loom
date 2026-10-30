import { useStore } from "jotai";
import { useLayoutEffect, type JSX } from "react";
import { useInRouterContext, useLocation } from "react-router";

import { offerErrorAtom } from "./offer-error";
import { screenAtom, screenOfPath } from "./session-home";

function RoutedScreen(): null {
  const store = useStore();
  const { pathname } = useLocation();
  const { feature, plan } = screenOfPath(pathname);
  // Before paint, so a navigation never shows one frame of the previous screen's session.
  useLayoutEffect(() => {
    store.set(screenAtom, { feature, plan });
    // A refusal is about the screen it was raised on.
    store.set(offerErrorAtom, null);
  }, [store, feature, plan]);
  return null;
}

/** Keeps `screenAtom` on the current route; renders nothing, and does nothing outside a router. */
export function ScreenSync(): JSX.Element | null {
  return useInRouterContext() ? <RoutedScreen /> : null;
}
