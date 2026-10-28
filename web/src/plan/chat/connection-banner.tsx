import { atom, useAtomValue } from "jotai";
import { useEffect, useState, type JSX } from "react";

import {
  authoringAtom,
  connectionAtom,
  isHolderAtom,
  presenceAtom,
  serverInfoAtom,
  stoppingAtom,
} from "@/plan/state/atoms";

const bannerAtom = atom((get) => {
  const connection = get(connectionAtom);
  if (connection.phase === "unauthorized") return null;
  if (get(serverInfoAtom)?.stopping || get(stoppingAtom)) {
    return { message: "loom plan is stopping. The session is writing its notes.", holder: false };
  }
  if (connection.phase === "open" || (!connection.everOpen && connection.phase === "connecting"))
    return null;
  const holder = get(authoringAtom)?.holder;
  const presentHolder = get(presenceAtom)?.entries.find((entry) => entry.holder);
  // Retain the hello's holder identity while individual browser atoms catch up.
  const holding =
    get(isHolderAtom) ||
    (holder != null &&
      presentHolder?.public_id === holder.public_id &&
      get(serverInfoAtom)?.you.public_id === holder.public_id);
  return {
    message: `Connection lost · ${connection.phase}`,
    holder: holding,
  };
});

function ageLabel(lastMessageAt: number | null, now: number): string {
  if (lastMessageAt === null) return "not received yet";
  const seconds = Math.max(0, Math.floor((now - lastMessageAt) / 1000));
  if (seconds < 60) return `${seconds}s ago`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

export function useConnectionDim(): string | null {
  const banner = useAtomValue(bannerAtom);
  const { lastMessageAt } = useAtomValue(connectionAtom);
  const [now, setNow] = useState(Date.now);
  const showing = banner !== null;
  useEffect(() => {
    if (!showing) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [showing, lastMessageAt]);
  return showing ? `Last update ${ageLabel(lastMessageAt, now)}` : null;
}

export function ConnectionBanner(): JSX.Element | null {
  const banner = useAtomValue(bannerAtom);
  if (!banner) return null;
  return (
    <div role="status" className="chat-connection-banner" aria-atomic="true">
      <p>
        <span aria-hidden="true">△</span> {banner.message}
      </p>
      {banner.holder && (
        <p>
          If this browser stays disconnected for 10 minutes, the session writes its notes and ends.
        </p>
      )}
    </div>
  );
}
