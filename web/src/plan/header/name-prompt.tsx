import { useAtomValue } from "jotai/react";
import { useEffect, useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { planSocketAtom } from "@/plan/state/atoms";

const NAME_KEY = "loom-plan-name";
/** Matches the server's cap (`plan_server/presence.rs`); it truncates anything longer. */
const MAX_NAME_CHARS = 32;

export function readStoredName(): string | null {
  try {
    return window.localStorage.getItem(NAME_KEY);
  } catch {
    return null;
  }
}

function writeStoredName(name: string): void {
  try {
    window.localStorage.setItem(NAME_KEY, name);
  } catch {
    // Storage can be blocked; the name then lasts only for this connection.
  }
}

/** Re-sends the stored name on every `hello`, so a reconnect keeps it. */
export function useResendStoredName(): void {
  const socket = useAtomValue(planSocketAtom);
  useEffect(() => {
    if (!socket) return;
    return socket.subscribe("hello", () => {
      const name = readStoredName();
      if (name) socket.send({ type: "set_name", name });
    });
  }, [socket]);
}

/** Inline field asking for a name until one is stored. */
export function NamePrompt() {
  const socket = useAtomValue(planSocketAtom);
  const [stored, setStored] = useState(readStoredName);
  const [draft, setDraft] = useState("");
  if (stored) return null;

  const save = (event: FormEvent) => {
    event.preventDefault();
    const name = draft.trim();
    if (!name) return;
    writeStoredName(name);
    socket?.send({ type: "set_name", name });
    setStored(name);
  };

  return (
    <form onSubmit={save} className="flex items-center gap-2">
      <input
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        maxLength={MAX_NAME_CHARS}
        aria-label="Your name"
        placeholder="Your name"
        className="h-7 min-w-0 flex-1 rounded-md border border-border bg-background px-2 text-[13px] font-medium"
      />
      <Button type="submit" size="sm">
        Save
      </Button>
    </form>
  );
}
