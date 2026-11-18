import { useAtomValue } from "jotai/react";
import { useEffect, useRef } from "react";

import { connectionAtom } from "@/plan/state/atoms";

/** The one 401 screen of section 12; the only place its wording appears. */
export function Unauthorized() {
  const { everOpen } = useAtomValue(connectionAtom);
  const heading = useRef<HTMLHeadingElement>(null);
  // A 401 replaces the app mid-session and focus falls to <body>; focus the notice so it is announced.
  useEffect(() => {
    heading.current?.focus();
  }, []);
  return (
    <main className="flex h-dvh flex-col items-center justify-center gap-3 px-6 text-center">
      <h1 ref={heading} tabIndex={-1} className="font-display text-xl outline-none">
        Sign in required
      </h1>
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
