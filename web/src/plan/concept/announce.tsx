import { useEffect, useRef, useState, type JSX } from "react";

/** One polite announcement per arrival, at most once every ten seconds (section 05). */
export const ANNOUNCE_INTERVAL_MS = 10_000;

export type Arrival = { count: number; seq: number };

function messageFor(title: string, count: number): string {
  return `${title}: ${count} ${count === 1 ? "sentence" : "sentences"} updated`;
}

/**
 * A visually hidden polite live region. An arrival inside the ten-second window waits for the
 * window to end and then announces the latest one, so a burst of rebuilds is read once.
 */
export function Announcer({
  title,
  arrival,
}: {
  title: string;
  arrival: Arrival | null;
}): JSX.Element {
  const [spoken, setSpoken] = useState<{ text: string; seq: number } | null>(null);
  const lastAt = useRef<number | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latest = useRef({ title });
  latest.current = { title };

  useEffect(() => {
    if (!arrival) return;
    const say = () => {
      lastAt.current = Date.now();
      timer.current = null;
      setSpoken({ text: messageFor(latest.current.title, arrival.count), seq: arrival.seq });
    };
    if (timer.current) clearTimeout(timer.current);
    const wait = lastAt.current === null ? 0 : lastAt.current + ANNOUNCE_INTERVAL_MS - Date.now();
    if (wait <= 0) say();
    else timer.current = setTimeout(say, wait);
  }, [arrival]);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  return (
    <div role="status" aria-live="polite" aria-atomic="true" className="sr-only">
      {spoken && <span key={spoken.seq}>{spoken.text}</span>}
    </div>
  );
}
