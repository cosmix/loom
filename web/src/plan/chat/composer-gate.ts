import type { SessionView } from "@/plan/api/schema-chat";

export function composerBlock(
  session: SessionView | null,
  {
    stopping,
    readOnly,
    codexAlone = false,
  }: {
    stopping: boolean;
    readOnly: boolean;
    codexAlone?: boolean;
  },
): string | null {
  if (stopping) return "loom plan is stopping";
  if (session?.state === "ending") return "the chat is closing";
  if (codexAlone) return "Codex is reviewing; send when the next Claude step starts";
  if (readOnly) return "loom plan is in read-only mode";
  return null;
}
