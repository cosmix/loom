import { useEffect, useState } from "react";

import { getFetchPolicy, saveFetchPolicy, type FetchPolicy } from "@/plan/api/schema-web-fetch";

export type Load =
  | { status: "loading" }
  | { status: "ready"; policy: FetchPolicy }
  | { status: "error"; message: string };

/** Saves the whole policy; true once the server holds it. */
export type Save = (policy: FetchPolicy) => Promise<boolean>;

const CHANGED_ELSEWHERE =
  "The web fetch settings changed since they were loaded (another tab, or Always allow on a card). They are reloaded below; check them and try again.";

async function loadPolicy(signal?: AbortSignal): Promise<Load> {
  const result = await getFetchPolicy(signal);
  return result.ok
    ? { status: "ready", policy: result.value }
    : { status: "error", message: result.message };
}

/** The project's fetch policy as the server holds it, with the save that changes it. */
export function usePolicy() {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const abort = new AbortController();
    void loadPolicy(abort.signal).then((next) => {
      if (!abort.signal.aborted) setLoad(next);
    });
    return () => abort.abort();
  }, [attempt]);
  const save: Save = async (policy) => {
    setSaving(true);
    setError(null);
    const result = await saveFetchPolicy(policy);
    setSaving(false);
    if (!result.ok) {
      // The policy moved on since it was read: show the server's copy, never overwrite it.
      if (result.status === 409) setLoad(await loadPolicy());
      setError(result.status === 409 ? CHANGED_ELSEWHERE : result.message);
      return false;
    }
    setLoad({ status: "ready", policy: result.value });
    return true;
  };
  const retry = () => {
    setLoad({ status: "loading" });
    setAttempt((count) => count + 1);
  };
  return { load, error, saving, save, retry };
}
