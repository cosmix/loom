import { useAtomValue } from "jotai/react";
import { Check, ChevronsUpDown } from "lucide-react";
import { useRef, useState, type KeyboardEvent } from "react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { activeFeatureAtom, featuresAtom, serverInfoAtom } from "@/plan/state/atoms";

const LINK_CLASS =
  "flex min-w-0 items-center justify-between gap-3 rounded-md px-2 py-1.5 hover:bg-accent focus-visible:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring";

function moveFocus(event: KeyboardEvent<HTMLElement>) {
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
  const links = Array.from(event.currentTarget.querySelectorAll<HTMLAnchorElement>("a[href]"));
  if (links.length === 0) return;

  event.preventDefault();
  const current = links.findIndex(
    (link) => link === event.currentTarget.ownerDocument.activeElement,
  );
  let next = event.key === "Home" ? 0 : links.length - 1;
  if (event.key === "ArrowDown") next = (current + 1) % links.length;
  if (event.key === "ArrowUp" && current >= 0) {
    next = (current - 1 + links.length) % links.length;
  }
  links[next]?.focus();
}

function FeatureLinks({ onSelect }: { onSelect: () => void }) {
  const features = useAtomValue(featuresAtom);
  const activeFeature = useAtomValue(activeFeatureAtom);

  return (
    <nav aria-label="Feature navigation" className="flex flex-col gap-1" onKeyDown={moveFocus}>
      {features.map(({ feature }) => (
        <Link
          key={feature}
          to={`/f/${encodeURIComponent(feature)}`}
          aria-current={feature === activeFeature ? "true" : undefined}
          className={LINK_CLASS}
          onClick={onSelect}
        >
          <span className="truncate">
            <Untrusted text={feature} />
          </span>
          {feature === activeFeature && <Check className="size-4 shrink-0" aria-hidden="true" />}
        </Link>
      ))}
      {features.length > 0 && <hr className="my-1 border-hairline" />}
      <Link to="/" className={LINK_CLASS} onClick={onSelect}>
        Home
      </Link>
      <Link to="/new" className={LINK_CLASS} onClick={onSelect}>
        New work…
      </Link>
    </nav>
  );
}

export function FeatureSwitcher() {
  const activeFeature = useAtomValue(activeFeatureAtom);
  const serverInfo = useAtomValue(serverInfoAtom);
  const [open, setOpen] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);
  const title = activeFeature ?? serverInfo?.project ?? "Project";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={`Switch feature, current: ${visibleControls(title)}`}
          className="flex max-w-full min-w-0 items-center gap-1 rounded-md text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          <span className="truncate font-display text-xl leading-tight font-semibold tracking-tight">
            <Untrusted text={title} />
          </span>
          <ChevronsUpDown className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <PopoverContent
        ref={contentRef}
        aria-label="Switch feature"
        align="start"
        className="gap-0 p-1"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          contentRef.current?.querySelector<HTMLAnchorElement>("a[href]")?.focus();
        }}
      >
        <FeatureLinks onSelect={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}
