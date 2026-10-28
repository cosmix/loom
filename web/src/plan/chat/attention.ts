import { useStore } from "jotai";
import { useEffect } from "react";

import type { ChatSnapshot, Item } from "@/plan/api/schema-chat";
import { isHolderAtom } from "@/plan/state/atoms";
import { notificationSupport, notificationsEnabledAtom } from "@/state/notifications";
import type { Store } from "@/state/store";
import { pendingPermissionsAtom, snapshotAtom } from "./model";
import { visibleControls } from "./visible-controls";

function notify(store: Store, title: string, body: string, tag: string): void {
  if (
    !store.get(isHolderAtom) ||
    store.get(notificationsEnabledAtom) !== true ||
    notificationSupport() !== "granted"
  )
    return;
  try {
    const notification = new window.Notification(title, { body, tag });
    notification.onclick = () => {
      window.focus();
      notification.close();
    };
  } catch {
    // Mobile browsers may require a service worker, as in dashboard notifications.
  }
}

function endedTurn(previous: ChatSnapshot, next: ChatSnapshot): string | null {
  if (!next.session || previous.session?.id !== next.session.id) return null;
  const open = new Set(previous.turns.filter((turn) => turn.open).map((turn) => turn.id));
  return next.turns.find((turn) => !turn.open && open.has(turn.id))?.id ?? null;
}

function notifyNewPermissions(
  store: Store,
  pending: Extract<Item, { type: "permission" }>[],
  seen: Set<string>,
): void {
  for (const item of pending) {
    if (!seen.has(item.request_id))
      notify(
        store,
        "Chat needs a decision",
        `Needs a decision: ${visibleControls(item.tool)}`,
        `chat:permission:${item.request_id}`,
      );
  }
}

/** Notifies when a turn closed while the tab was hidden; true when it did. */
function notifyTurnEnd(store: Store, previous: ChatSnapshot, next: ChatSnapshot): boolean {
  const ended = endedTurn(previous, next);
  if (!ended || !next.session || !document.hidden) return false;
  notify(store, "Chat turn ended", "Waiting for you", `chat:turn:${next.session.id}:${ended}`);
  return true;
}

function watchAttention(store: Store): () => void {
  const baseTitle = document.title;
  let previous = store.get(snapshotAtom);
  let seen = new Set(store.get(pendingPermissionsAtom).map((item) => item.request_id));
  let acknowledged = new Set<string>();
  let completed = false;
  const title = () => {
    const waiting = store
      .get(pendingPermissionsAtom)
      .some((item) => !acknowledged.has(item.request_id));
    document.title = `${waiting ? "(!) " : completed ? "(✓) " : ""}${baseTitle}`;
  };
  const update = () => {
    const next = store.get(snapshotAtom);
    const pending = store.get(pendingPermissionsAtom);
    notifyNewPermissions(store, pending, seen);
    if (notifyTurnEnd(store, previous, next)) completed = true;
    previous = next;
    seen = new Set(pending.map((item) => item.request_id));
    title();
  };
  const visible = () => {
    if (document.hidden) return;
    completed = false;
    acknowledged = new Set(store.get(pendingPermissionsAtom).map((item) => item.request_id));
    title();
  };
  title();
  const unsubscribe = store.sub(snapshotAtom, update);
  // The empty listener mounts the storage-backed atom so changes made in other tabs arrive.
  const unwatch = store.sub(notificationsEnabledAtom, () => {});
  document.addEventListener("visibilitychange", visible);
  return () => {
    unsubscribe();
    unwatch();
    document.removeEventListener("visibilitychange", visible);
    document.title = baseTitle;
  };
}

export function useAttention(): void {
  const store = useStore();
  useEffect(() => watchAttention(store), [store]);
}
