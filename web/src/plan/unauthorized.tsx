import { useAtomValue } from "jotai/react";

import { connectionAtom } from "@/plan/state/atoms";

/** The one 401 screen of section 12; the only place its wording appears. */
export function Unauthorized() {
  const { everOpen } = useAtomValue(connectionAtom);
  return (
    <main className="flex h-dvh flex-col items-center justify-center gap-3 px-6 text-center">
      <h1 className="font-display text-xl">Sign in required</h1>
      {everOpen && (
        <p className="max-w-md text-sm">
          loom plan restarted. Open the URL it printed in the terminal.
        </p>
      )}
      <p className="max-w-md text-sm text-muted-foreground">
        Open the link <code>loom plan</code> printed. Running <code>loom plan</code> again in this
        project prints it.
      </p>
    </main>
  );
}
