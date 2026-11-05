import { useAtomValue } from "jotai/react";
import { useId, useState, type ReactNode } from "react";
import { Link, Navigate } from "react-router";

import type { CandidateView, ChecklistView, OfferView, SpecItemView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { builtOf, followUpSeed, isBuilt, reviewFile, reviewHref } from "@/plan/feature/built-model";
import { landingPath } from "@/plan/feature/landing";
import { offerOfKind } from "@/plan/feature/page-offer";
import { PlanPage } from "@/plan/feature/plan-page";
import { PlanName } from "@/plan/feature/plan-state";
import { useFeature } from "@/plan/feature/use-feature";
import { offerAction, runOffer } from "@/plan/offers/actions";
import { authorBlockReasonAtom } from "@/plan/state/atoms";
import { DisabledReason } from "@/plan/ui/disabled-reason";
import { LoadState } from "@/plan/ui/load-state";

import "./built.css";

const LEDE =
  "Every plan is done. What the runs left open is below; each item can become a follow-up entry in the metaplan.";

/** A section of the page, named by its title for assistive technology. */
function Section({ title, children }: { title: string; children: ReactNode }) {
  const id = useId();
  return (
    <section aria-labelledby={id}>
      <h2 id={id} className="plan-section-title">
        {title}
      </h2>
      {children}
    </section>
  );
}

/** Where an entry came from: the stage's name and its review file, which opens the review. */
function SourceLine({ stage, review }: { stage: string; review: string }) {
  return (
    <p className="plan-fine plan-built__source">
      <Untrusted text={stage} />
      {" · "}
      <Link to={reviewHref(review)} className="plan-page__link">
        <Untrusted text={reviewFile(review)} />
      </Link>
    </p>
  );
}

/** Every done plan: its slug, its title and the run review that closed it, when one was written. */
function DonePlans({ checklist }: { checklist: ChecklistView }) {
  const reviews = new Map(builtOf(checklist).reviews.map((review) => [review.slug, review.path]));
  const rows = checklist.plan_rows.filter((row) => row.state === "done");
  return (
    <ul aria-label="Done plans" className="plan-list plan-page__rows plan-built__plans">
      {rows.map((row) => {
        const review = reviews.get(row.slug);
        return (
          <li key={row.path}>
            <PlanName feature={checklist.feature} slug={row.slug} />
            <span className="plan-built__title">
              <Untrusted text={row.title} />
            </span>
            <span className="plan-page__side">
              {review === undefined ? (
                <span className="plan-fine">No run review</span>
              ) : (
                <Link to={reviewHref(review)} className="plan-page__link">
                  Run review
                </Link>
              )}
            </span>
          </li>
        );
      })}
    </ul>
  );
}

/** What integration-verify changed so the spec and the build agree, one entry per review item. */
function SpecCheck({ items }: { items: SpecItemView[] }) {
  if (items.length === 0) {
    return <p className="plan-empty">The runs recorded no change to the spec.</p>;
  }
  return (
    <ul aria-label="Spec check" className="plan-list plan-built__entries">
      {items.map((item, index) => (
        <li key={`${index}:${item.stage_id}`}>
          <p className="plan-built__text">
            <Untrusted text={item.text} />
          </p>
          <SourceLine stage={item.stage} review={item.review} />
        </li>
      ))}
    </ul>
  );
}

/**
 * One open finding or question, with the action that seeds a Plan follow-ups session with it.
 * The button is drawn only for a usable offer (`Candidates` decides), under the authoring block
 * as every offer button is.
 */
function CandidateRow({ candidate, offer }: { candidate: CandidateView; offer: OfferView | null }) {
  const reason = useAtomValue(authorBlockReasonAtom);
  const textId = useId();
  return (
    <li>
      <p className="plan-fine plan-built__kind">
        {candidate.kind === "finding" ? "Open finding" : "Open question"}
      </p>
      <p id={textId} className="plan-built__text">
        <Untrusted text={candidate.text} />
      </p>
      <SourceLine stage={candidate.stage} review={candidate.review} />
      {offer !== null && (
        <div className="plan-built__action">
          <DisabledReason
            reason={reason}
            size="sm"
            describedBy={textId}
            onClick={() => runOffer(offer, followUpSeed(candidate))}
          >
            Plan this
          </DisabledReason>
        </div>
      )}
    </li>
  );
}

function Candidates({
  candidates,
  offer,
}: {
  candidates: CandidateView[];
  offer: OfferView | null;
}) {
  if (candidates.length === 0) {
    return <p className="plan-empty">The run reviews list no open findings or questions.</p>;
  }
  return (
    <ul aria-label="Candidate follow-ups" className="plan-list plan-built__entries">
      {candidates.map((candidate, index) => (
        <CandidateRow key={`${index}:${candidate.stage_id}`} candidate={candidate} offer={offer} />
      ))}
    </ul>
  );
}

/** The way to the kickoff screen, which can start a follow-up for this feature from scratch. */
function NewWorkLink({ feature }: { feature: string }) {
  return (
    <Link to="/new" state={{ followUp: feature }} className="plan-page__link plan-built__aside">
      Start it on New work
    </Link>
  );
}

/**
 * Free text for a follow-up the reviews did not list: the screen's one primary sends it as the
 * first message of a Plan follow-ups session. With no usable offer, only the way to New work.
 */
function SomethingElse({ feature, offer }: { feature: string; offer: OfferView | null }) {
  const block = useAtomValue(authorBlockReasonAtom);
  const [text, setText] = useState("");
  const id = useId();
  if (offer === null) {
    return (
      <div className="plan-built__free">
        <p className="plan-empty">Describe the follow-up on New work.</p>
        <NewWorkLink feature={feature} />
      </div>
    );
  }
  const trimmed = text.trim();
  const reason = block ?? (trimmed === "" ? "Describe the follow-up" : null);
  return (
    <div className="plan-built__free">
      <label htmlFor={id} className="plan-built__label">
        What else needs doing?
      </label>
      <textarea
        id={id}
        rows={3}
        value={text}
        onChange={(event) => setText(event.target.value)}
        className="plan-built__field"
      />
      <div className="plan-built__actions">
        <DisabledReason
          look="primary"
          reason={reason}
          onClick={() => {
            runOffer(offer, trimmed);
            setText("");
          }}
        >
          <Untrusted text={offer.label} />
        </DisabledReason>
        <NewWorkLink feature={feature} />
      </div>
    </div>
  );
}

/** The Plan follow-ups offer when the checklist carries it and an action is registered for it. */
function followUpOffer(checklist: ChecklistView): OfferView | null {
  const offer = offerOfKind(checklist, "plan_follow_ups");
  return offer !== undefined && offerAction(offer.kind) !== undefined ? offer : null;
}

/**
 * `/f/:feature/built`: what a finished feature's runs produced and left open. An unfinished
 * feature has no Built state and goes where it would land.
 */
export function BuiltPage() {
  const { checklist } = useFeature();
  if (checklist === undefined) return <LoadState state="loading" title="Loading the feature" />;
  if (!isBuilt(checklist)) return <Navigate to={landingPath(checklist)} replace />;
  const built = builtOf(checklist);
  const offer = followUpOffer(checklist);
  return (
    <PlanPage feature={checklist.feature} label="built" title="Built" lede={LEDE}>
      <div className="plan-built">
        <Section title="Done plans">
          <DonePlans checklist={checklist} />
        </Section>
        <Section title="Spec check">
          <SpecCheck items={built.spec} />
        </Section>
        <Section title="Candidate follow-ups">
          <Candidates candidates={built.candidates} offer={offer} />
        </Section>
        <Section title="Something else">
          <SomethingElse feature={checklist.feature} offer={offer} />
        </Section>
      </div>
    </PlanPage>
  );
}
