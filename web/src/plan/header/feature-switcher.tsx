import { useAtomValue } from "jotai/react";
import { Check } from "lucide-react";
import { useRef, useState, type KeyboardEvent } from "react";
import { Link, useLocation, useMatch, useSearchParams } from "react-router";

import { featureOfKey, pageKeyOfPath } from "@/plan/api/page-key";
import { sessionAtom } from "@/plan/chat/model";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { kindOf } from "@/plan/new/kickoff-state";
import { featuresAtom, kickoffAtom, kickoffDraftsAtom, serverInfoAtom } from "@/plan/state/atoms";
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

type Identity = {
  /** The feature the route names, if any. */
  feature: string | undefined;
  eyebrow: string;
  title: string;
};

function isFilesRoute(pathname: string): boolean {
  return ["/files", "/plans"].some((root) => pathname === root || pathname.startsWith(`${root}/`));
}

const NEW_EYEBROW = { feature: "Planning · new feature", plan: "Planning · new plan" } as const;

/**
 * The work `/new?session=<id>` shows: the live kickoff when it is that session, else its draft.
 * `/new` alone is an empty form, so it names no one's work.
 */
function useNewWork(): { name: string | null; size: "feature" | "plan" | null } {
  const [params] = useSearchParams();
  const session = params.get("session");
  const kickoff = useAtomValue(kickoffAtom);
  const chat = useAtomValue(sessionAtom);
  const drafts = useAtomValue(kickoffDraftsAtom);
  if (session === null) return { name: null, size: null };
  if (kickoff?.session === session) {
    // Before the conversation settles a size, the kickoff's start offer says it, as on the page head.
    const offer = chat?.id === session ? chat.offer : null;
    return { name: kickoff.shape.name, size: kindOf(kickoff.shape.size, offer) };
  }
  return drafts.find((draft) => draft.session === session) ?? { name: null, size: null };
}

/**
 * What the app bar names: the route's feature (by slug), the work `/new?session=` continues (by the
 * name it has so far, the rename included), or the project.
 */
function useIdentity(): Identity {
  const { pathname } = useLocation();
  const featureRoute = useMatch("/f/:feature/*");
  const serverInfo = useAtomValue(serverInfoAtom);
  const work = useNewWork();
  const pageKey = pageKeyOfPath(pathname);
  const feature = (pageKey === null ? null : featureOfKey(pageKey)) ?? featureRoute?.params.feature;
  if (feature !== undefined) {
    return { feature, eyebrow: "Planning · feature", title: feature };
  }
  if (pathname === "/new") {
    return {
      feature,
      eyebrow: work.size === null ? "Planning · new work" : NEW_EYEBROW[work.size],
      title: work.name || "Untitled",
    };
  }
  return {
    feature,
    eyebrow: isFilesRoute(pathname) ? "Planning · files" : "Planning · project",
    title: serverInfo?.project ?? "Project",
  };
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
  if (features.length === 0) {
    return <LoadState state="empty" size="compact" title="No features yet." />;
  }
  return (
    <nav aria-label="Feature navigation" className="plan-switcher-menu" onKeyDown={moveFocus}>
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

/**
 * The identity as one button: the eyebrow over the title and its menu chevron share one hover
 * surface. The accessible name is the label, so the eyebrow is not read twice.
 */
export function FeatureSwitcher() {
  const { feature, eyebrow, title } = useIdentity();
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
          <span className="plan-eyebrow">{eyebrow}</span>
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
          contentRef.current?.querySelector<HTMLAnchorElement>("a[href]")?.focus();
        }}
      >
        <p className="plan-eyebrow plan-switcher-head">Features</p>
        <FeatureLinks currentFeature={feature} onSelect={() => setOpen(false)} />
      </PopoverContent>
    </Popover>
  );
}

/** The app bar's identity cell: the switcher, eyebrow included. */
export function Identity() {
  return (
    <div className="plan-identity">
      <FeatureSwitcher />
    </div>
  );
}
