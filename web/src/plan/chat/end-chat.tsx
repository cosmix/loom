import { useAtomValue } from "jotai";

import { isHolderAtom } from "@/plan/state/atoms";
import { isActiveSession, panelSessionAtom } from "./session-home";

/**
 * Whether this browser may end the chat the panel shows: never one that belongs to another screen,
 * which the panel presents as no chat. Pressure-test sessions end on their own.
 */
export function useCanEndChat(): boolean {
  const session = useAtomValue(panelSessionAtom);
  const holder = useAtomValue(isHolderAtom);
  return holder && isActiveSession(session) && session.kind !== "pressure";
}
