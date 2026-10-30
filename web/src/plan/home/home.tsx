import { useAtomValue, useStore } from "jotai";
import { MessageCircle } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { Link, Navigate, useLocation } from "react-router";

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

import "./home.css";

let autoOpened = false;

export function resetHomeAutoOpen() {
  autoOpened = false;
}

function StartConversation() {
  const canAuthor = useAtomValue(canAuthorAtom);
  const blockReason = useAtomValue(authorBlockReasonAtom);

  if (canAuthor) {
    return (
      <Link to="/new" className="invitation__cta">
        <MessageCircle aria-hidden="true" className="size-4" />
        Start a conversation
      </Link>
    );
  }

  return (
    <div className="grid gap-1">
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

export function InvitationCard({ empty }: { empty: boolean }) {
  const headingId = useId();

  return (
    <section aria-labelledby={headingId} data-doodle-host className="invitation">
      <div className="invitation__art">
        <LoomDoodle />
      </div>
      <div className="invitation__body">
        <h2 id={headingId} className="invitation__title">
          Start something new
        </h2>
        <div className="invitation__lead">
          {empty && <p className="invitation__empty">No features yet.</p>}
          <p>
            Bring a rough idea. Loom talks it through with you, then shapes it into a feature with
            its own spec, or a single plan.
          </p>
        </div>
        <StartConversation />
        <p className="invitation__note">Nothing is written until you say so.</p>
      </div>
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
    return location.key === "default" && !autoOpened && features.length === 1 && feature
      ? `/f/${encodeURIComponent(feature.feature)}`
      : null;
  });

  useEffect(() => {
    // Consume navigation arrivals too, after commit so Strict Mode keeps its destination.
    autoOpened = true;
  }, []);

  if (destination) return <Navigate to={destination} replace />;

  const invitation = <InvitationCard empty={empty} />;

  return (
    <div className="home">
      {!empty && (
        <div className="home__head">
          <p className="home__eyebrow">doc/ in this repository</p>
          <h2 className="home__title">Features</h2>
        </div>
      )}
      {features.length > 0 ? (
        <FeatureCards lead={invitation} />
      ) : (
        <div className="card-grid">
          <div className="card-grid__lead">{invitation}</div>
        </div>
      )}
      {!empty && <StandaloneShelf />}
    </div>
  );
}

export function HomeRoute() {
  const project = useAtomValue(projectAtom);
  if (project === null) return <LoadState state="loading" title="Loading your project" />;
  return <LoadedHome />;
}
