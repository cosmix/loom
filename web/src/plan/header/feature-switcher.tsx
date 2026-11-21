import { useAtomValue } from "jotai/react";
import { Check } from "lucide-react";
import { useRef, useState, type KeyboardEvent } from "react";
import { Link, useLocation, useMatch } from "react-router";

import { featureOfKey, pageKeyOfPath } from "@/plan/api/page-key";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { featuresAtom } from "@/plan/state/atoms";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";
import { LoadState } from "@/plan/ui/load-state";

const SwitcherIcon = OTHER_GLYPHS.switcher;

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

function useRouteFeature() {
  const { pathname } = useLocation();
  const featureRoute = useMatch("/f/:feature/*");
  const pageKey = pageKeyOfPath(pathname);
  return (pageKey === null ? null : featureOfKey(pageKey)) ?? featureRoute?.params.feature;
}

/** The features, by slug; the route's one carries `aria-current` and a check. */
function FeatureLinks({
  currentFeature,
  onSelect,
}: {
  currentFeature: string | undefined;
  onSelect: () => void;
}) {
  const features = useAtomValue(featuresAtom);
  const home = useMatch("/") !== null;
  return (
    <nav aria-label="Feature navigation" className="plan-switcher-menu" onKeyDown={moveFocus}>
      <Link
        to="/"
        className="plan-switcher-item"
        onClick={onSelect}
        aria-current={home ? "page" : undefined}
      >
        Home
        {home && <Check aria-hidden="true" />}
      </Link>
      {features.length === 0 && <LoadState state="empty" size="compact" title="No features yet." />}
      {features.map(({ feature }) => (
        <Link
          key={feature}
          to={`/f/${encodeURIComponent(feature)}`}
          aria-current={feature === currentFeature ? "true" : undefined}
          className="plan-switcher-item"
          onClick={onSelect}
        >
          <span className="plan-switcher-slug font-mono">
            <Untrusted text={feature} />
          </span>
          {feature === currentFeature && <Check aria-hidden="true" />}
        </Link>
      ))}
    </nav>
  );
}

export function FeatureSwitcher() {
  const feature = useRouteFeature();
  const title = feature ?? "Home";
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
          <span className="plan-switcher-row">
            <span className="plan-switcher-title">
              <Untrusted text={title} />
            </span>
            <SwitcherIcon className="plan-switcher-chevron" aria-hidden="true" />
          </span>
        </button>
      </PopoverTrigger>
      <PopoverContent
        ref={contentRef}
        aria-label="Switch feature"
        align="start"
        className="w-64 gap-1 p-1.5"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          const menu = contentRef.current;
          const link =
            menu?.querySelector<HTMLAnchorElement>("a[aria-current]") ??
            menu?.querySelector<HTMLAnchorElement>("a[href]");
          link?.focus();
        }}
      >
        <FeatureLinks currentFeature={feature} onSelect={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}

/** The app bar's identity cell. */
export function Identity() {
  return (
    <div className="plan-identity">
      <FeatureSwitcher />
    </div>
  );
}
