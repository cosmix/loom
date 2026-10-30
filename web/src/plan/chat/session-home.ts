import { atom } from "jotai";

import type { SessionView } from "@/plan/api/schema-chat";
import { featuresAtom, standaloneAtom } from "@/plan/state/atoms";
import { sessionAtom } from "./model";
import { shownSessionAtom } from "./shown-session";

/** What the routed screen is about. Both null: a screen no feature or plan owns. */
export type Screen = { feature: string | null; plan: string | null };

const NO_OWNER: Screen = { feature: null, plan: null };
const PLAN_FILE = /^(?:(?:IN_PROGRESS|DONE|RETIRED)-)?PLAN-(.+)\.md$/;

/**
 * The routed screen, written by `ScreenSync`. Null outside a router (help samples, a bare
 * composer): such a mount has no screen to disagree with, so it shows the session it is given.
 */
export const screenAtom = atom<Screen | null>(null);

function planSlug(path: string): string | null {
  return PLAN_FILE.exec(path.split("/").pop() ?? "")?.[1] ?? null;
}

/** The owner of a document, from its path under `doc/` (`planning/kinds.rs` lays them out). */
function screenOfDoc([root, ...rest]: string[]): Screen {
  if (root === "product") return rest.length > 1 ? { feature: rest[0], plan: null } : NO_OWNER;
  if (root !== "plans" || rest.length === 0) return NO_OWNER;
  const archived = rest[0] === "archive";
  const home = archived ? rest.slice(1) : rest;
  if (home.length === 1) return { feature: null, plan: planSlug(home[0]) };
  // A standalone plan's bundle is named by its slug; `archive` and `briefs` are never features.
  if (home[0] === "briefs") return home.length > 2 ? { feature: null, plan: home[1] } : NO_OWNER;
  return archived ? NO_OWNER : { feature: rest[0], plan: null };
}

export function screenOfPath(pathname: string): Screen {
  let segments: string[];
  try {
    segments = pathname.split("/").filter(Boolean).map(decodeURIComponent);
  } catch {
    return NO_OWNER;
  }
  const [route, name = null, ...rest] = segments;
  if (route === "f") return { feature: name, plan: null };
  if (route === "plans") return { feature: null, plan: name };
  if (route === "files" && name !== null) return screenOfDoc([name, ...rest]);
  return NO_OWNER;
}

/** What a chat started from this screen belongs to. */
export type StartOwner = { feature: string | undefined; plan: string | undefined };

/**
 * The screen's owner as a start may name it. The route's name is unchecked (`/f/<anything>`) and
 * the server refuses a start for a name it does not know, so only a listed name is sent.
 */
export const startOwnerAtom = atom<StartOwner>((get) => {
  const screen = get(screenAtom);
  return {
    feature: get(featuresAtom).find((item) => item.feature === screen?.feature)?.feature,
    plan: get(standaloneAtom).find((item) => item.slug === screen?.plan)?.slug,
  };
});

/** Where a session belongs: the place its notice links to and its title in the window title. */
export type SessionHome = {
  feature: string | null;
  plan: string | null;
  title: string;
  href: string;
};

/**
 * The feature or standalone plan a session belongs to. Null for a chat started with neither (a
 * kickoff before its confirmation, a New chat off those routes): it belongs to the whole project,
 * so it stays on every screen.
 */
export const sessionHomeAtom = atom<SessionHome | null>((get) => {
  const session = get(shownSessionAtom);
  if (!session) return null;
  if (session.feature !== null) {
    const { feature } = session;
    const title = get(featuresAtom).find((item) => item.feature === feature)?.title ?? feature;
    return { feature, plan: null, title, href: `/f/${encodeURIComponent(feature)}` };
  }
  const { plan } = session;
  if (plan === null) return null;
  const title = get(standaloneAtom).find((item) => item.slug === plan)?.title ?? plan;
  return { feature: null, plan, title, href: `/plans/${encodeURIComponent(plan)}` };
});

/** Whether this screen shows the session: its feature's routes and documents, or its plan's. */
export const sessionHereAtom = atom((get) => {
  const home = get(sessionHomeAtom);
  const screen = get(screenAtom);
  if (home === null || screen === null) return true;
  return home.feature !== null ? screen.feature === home.feature : screen.plan === home.plan;
});

export function isActiveSession(session: SessionView | null): session is SessionView {
  return session !== null && session.state !== "ended" && session.state !== "failed";
}

/**
 * The open session this screen does not show. The server runs one chat at a time, so a start from
 * here cannot succeed; ending a session the user cannot see is not a question to ask them here.
 * A concluded session (a `previous` one still shown) is not open, so it is never elsewhere.
 */
export const elsewhereSessionAtom = atom<SessionHome | null>((get) =>
  isActiveSession(get(sessionAtom)) && !get(sessionHereAtom) ? get(sessionHomeAtom) : null,
);

/** The session the panel presents here; a session that belongs elsewhere reads as no session. */
export const panelSessionAtom = atom((get) =>
  get(sessionHereAtom) ? get(shownSessionAtom) : null,
);
