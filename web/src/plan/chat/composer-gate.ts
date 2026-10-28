import type { SessionView } from "@/plan/api/schema-chat";

export function composerBlock(
  session: SessionView | null,
  { stopping, readOnly }: { stopping: boolean; readOnly: boolean },
): string | null {
  if (stopping) return "loom plan is stopping";
  if (session?.state === "ending") return "the chat is closing";
  if (readOnly) return "loom plan is in read-only mode";
  return null;
}
