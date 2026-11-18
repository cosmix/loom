import { cn } from "cn";
import { useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";

/// Whether the box's content is larger than the box, re-measured after every
/// render (the text may have changed) and whenever the box resizes.
function useOverflows(box: RefObject<HTMLElement | null>): boolean {
  const [overflows, setOverflows] = useState(false);
  useLayoutEffect(() => {
    const element = box.current;
    if (element) setOverflows(element.scrollHeight > element.clientHeight);
  });
  useLayoutEffect(() => {
    const element = box.current;
    if (!element) return;
    const observer = new ResizeObserver(() =>
      setOverflows(element.scrollHeight > element.clientHeight),
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [box]);
  return overflows;
}

/// Long untrusted text in a bordered box that scrolls past 288px instead of
/// cutting anything. Only a box that scrolls is a named, focusable region, so
/// a keyboard can reach and scroll it; a short one is plain text.
export function TextBox({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const scrolls = useOverflows(box);
  return (
    <div
      ref={box}
      tabIndex={scrolls ? 0 : undefined}
      role={scrolls ? "region" : undefined}
      aria-label={scrolls ? label : undefined}
      className={cn("text-box", className)}
    >
      {children}
    </div>
  );
}

/// Evidence or record lines, one per row, each whole. Text nodes only.
export function LinesBox({ label, lines }: { label: string; lines: string[] }) {
  return (
    <TextBox label={label}>
      <pre className="text-box-lines">
        {lines.map((line, index) => (
          <span key={index} className="block">
            {line}
          </span>
        ))}
      </pre>
    </TextBox>
  );
}
