import type { SessionView } from "@/plan/api/schema-chat";
import { PressureHeader } from "./pressure/pressure-header";

/** The run line under the head: a pressure test's own head, and nothing for an authoring chat. */
export function RunHeader({ session }: { session: SessionView }) {
  return session.kind === "pressure" ? <PressureHeader session={session} /> : null;
}
