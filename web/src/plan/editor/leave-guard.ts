import { useEffect } from "react";
import { useBlocker } from "react-router";

export function useLeaveGuard(dirty: boolean): void {
  const blocker = useBlocker(dirty);

  useEffect(() => {
    if (blocker.state !== "blocked") return;
    if (dirty && window.confirm("You have unsaved changes. Leave this page?")) {
      blocker.proceed();
    } else {
      blocker.reset();
    }
  }, [blocker, dirty]);

  useEffect(() => {
    if (!dirty) return;
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [dirty]);
}
