import { useAtomValue, useStore } from "jotai";
import { useEffect, useId, useState } from "react";
import { Link, Navigate, useLocation } from "react-router";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useFileSocket } from "@/plan/files/api";
import { FeatureCards } from "@/plan/home/feature-card";
import { StandaloneShelf } from "@/plan/home/standalone-shelf";
import {
  authorBlockReasonAtom,
  canAuthorAtom,
  featuresAtom,
  projectAtom,
  standaloneAtom,
} from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoomDoodle } from "@/plan/ui/doodle";
import { LoadState } from "@/plan/ui/load-state";

let autoOpened = false;

export function resetHomeAutoOpen() {
  autoOpened = false;
}

function StartConversation() {
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);

  if (canAuthor) {
    return (
      <Button asChild>
        <Link to="/new">Start a conversation</Link>
      </Button>
    );
  }

  return (
    <div className="space-y-1">
      <DisabledReason reason="Authoring is unavailable" onClick={() => {}}>
        Start a conversation
      </DisabledReason>
      {blockReason && (
        <p className="text-xs text-muted-foreground">
          <Untrusted text={blockReason} />
        </p>
      )}
    </div>
  );
}

function InvitationCard({ empty }: { empty: boolean }) {
  const headingId = useId();

  return (
    <section
      aria-labelledby={headingId}
      data-doodle-host
      className="flex flex-col items-start gap-5 rounded-xl border border-hairline bg-card p-6 text-card-foreground sm:p-8"
    >
      <LoomDoodle className="text-muted-foreground" />
      <h1 id={headingId} className="font-display text-3xl leading-tight font-semibold">
        Start something new
      </h1>
      <div className="max-w-xl space-y-2 text-sm leading-relaxed text-muted-foreground">
        {empty && <p className="font-medium text-foreground">No features yet.</p>}
        <p>
          Tell Loom what you want to build. It will help you decide whether it is a feature or a
          single plan.
        </p>
      </div>
      <StartConversation />
      <p className="text-xs text-muted-foreground">Nothing is written until you say so.</p>
    </section>
  );
}

function LoadedHome() {
  const store = useStore();
  useFileSocket(store);
  const location = useLocation();
  const features = useAtomValue(featuresAtom);
  const standalone = useAtomValue(standaloneAtom);
  const empty = features.length === 0 && standalone.length === 0;
  const [destination] = useState(() => {
    const [feature] = features;
    return location.key === "default" &&
      !autoOpened &&
      features.length === 1 &&
      standalone.length === 0 &&
      feature
      ? `/f/${encodeURIComponent(feature.feature)}`
      : null;
  });

  useEffect(() => {
    // Consume navigation arrivals too, after commit so Strict Mode keeps its destination.
    autoOpened = true;
  }, []);

  if (destination) return <Navigate to={destination} replace />;

  return (
    <div className="mx-auto w-full max-w-5xl space-y-8 px-4 py-8 sm:px-6">
      <InvitationCard empty={empty} />
      {!empty && (
        <>
          <FeatureCards />
          <StandaloneShelf />
        </>
      )}
    </div>
  );
}

export function HomeRoute() {
  const project = useAtomValue(projectAtom);
  if (project === null) return <LoadState state="loading" title="Loading your project" />;
  return <LoadedHome />;
}
