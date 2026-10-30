import { useAtomValue } from "jotai/react";
import { Check } from "lucide-react";
import { useRef, useState, type KeyboardEvent } from "react";
import { Link, useLocation, useMatch } from "react-router";

import { featureOfKey, pageKeyOfPath } from "@/plan/api/page-key";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { featuresAtom, serverInfoAtom } from "@/plan/state/atoms";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

const SwitcherIcon = OTHER_GLYPHS.switcher;

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

type Identity = {
  /** The feature the route names, if any. */
  feature: string | undefined;
  eyebrow: string;
  title: string;
};

function isFilesRoute(pathname: string): boolean {
  return ["/files", "/plans"].some((root) => pathname === root || pathname.startsWith(`${root}/`));
}

/** What the app bar names: the route's feature (by slug), the new-work draft, or the project. */
export function useIdentity(): Identity {
  const { pathname } = useLocation();
  const featureRoute = useMatch("/f/:feature/*");
  const serverInfo = useAtomValue(serverInfoAtom);
  const pageKey = pageKeyOfPath(pathname);
  const feature = (pageKey === null ? null : featureOfKey(pageKey)) ?? featureRoute?.params.feature;
  if (feature !== undefined) {
    return { feature, eyebrow: "Planning · feature", title: feature };
  }
  if (pathname === "/new") {
    return { feature, eyebrow: "Planning · new feature", title: "Untitled" };
  }
  return {
    feature,
    eyebrow: isFilesRoute(pathname) ? "Planning · files" : "Planning · project",
    title: serverInfo?.project ?? "Project",
  };
}

function FeatureLinks({
  currentFeature,
  onSelect,
}: {
  currentFeature: string | undefined;
  onSelect: () => void;
}) {
  const features = useAtomValue(featuresAtom);

  return (
    <nav aria-label="Feature navigation" className="flex flex-col gap-1" onKeyDown={moveFocus}>
      {features.map(({ feature }) => (
        <Link
          key={feature}
          to={`/f/${encodeURIComponent(feature)}`}
          aria-current={feature === currentFeature ? "true" : undefined}
          className={LINK_CLASS}
          onClick={onSelect}
        >
          <span className="truncate font-mono text-[13px] font-medium">
            <Untrusted text={feature} />
          </span>
          {feature === currentFeature && <Check className="size-4 shrink-0" aria-hidden="true" />}
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
  const { feature, title } = useIdentity();
  const [open, setOpen] = useState(false);
  const contentRef = useRef<HTMLDivElement>(null);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label={
            feature === undefined
              ? `${visibleControls(title)}, switch feature`
              : `Switch feature, current: ${visibleControls(feature)}`
          }
          className="plan-switcher"
        >
          <span className="plan-switcher-title">
            <Untrusted text={title} />
          </span>
          <SwitcherIcon className="size-4 shrink-0 text-muted-foreground" aria-hidden="true" />
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
        <FeatureLinks currentFeature={feature} onSelect={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}

/** The eyebrow over the switcher; the eyebrow names the route, the switcher its subject. */
export function Identity() {
  const { eyebrow } = useIdentity();
  return (
    <div className="plan-identity">
      <p className="plan-eyebrow">{eyebrow}</p>
      <FeatureSwitcher />
    </div>
  );
}
