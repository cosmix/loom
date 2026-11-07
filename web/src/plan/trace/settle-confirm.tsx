import { CircleHelp, Flag } from "lucide-react";
import type { JSX } from "react";
import { Link } from "react-router";

import { Button } from "@/components/ui/button";
import { Untrusted } from "@/plan/chat/visible-controls";
import { useKeptDoc } from "@/plan/concept/passage-doc";
import { featureSourceHref } from "@/plan/feature/source-href";
import { plural } from "@/plan/ui/plural";

import { settlePoints, type SettlePoint } from "./doc-model";

/** The most quotes the confirmation lists; the counts above them still say how many there are. */
const MAX_POINTS = 6;

/** "1 My call and 2 open questions": only the kinds the document holds. */
function pointsText(myCalls: number, openQuestions: number): string {
  const points = [
    ...(myCalls > 0 ? [plural(myCalls, "My call")] : []),
    ...(openQuestions > 0 ? [plural(openQuestions, "open question")] : []),
  ];
  return points.join(" and ");
}

function Where({
  point,
  feature,
  path,
  onNavigate,
}: {
  point: SettlePoint;
  feature: string;
  path: string;
  onNavigate: () => void;
}): JSX.Element {
  if (point.kind === "question") {
    return <>{`Open question ${point.index}`}</>;
  }
  if (!point.where) return <>Your call</>;
  const to = featureSourceHref(feature, path, { anchor: point.where.anchor });
  return (
    <>
      Your call · in{" "}
      <Link to={to} onClick={onNavigate}>
        <Untrusted text={point.where.title} />
      </Link>
    </>
  );
}

function PointRow(props: {
  point: SettlePoint;
  feature: string;
  path: string;
  onNavigate: () => void;
}): JSX.Element {
  const { point } = props;
  const Icon = point.kind === "call" ? Flag : CircleHelp;
  return (
    <li className="trace-confirm__point" data-kind={point.kind}>
      <span className="trace-confirm__icon" aria-hidden="true">
        <Icon />
      </span>
      <q className="trace-confirm__quote">
        <Untrusted text={point.quote} />
      </q>
      <span className="trace-confirm__where">
        <Where {...props} />
      </span>
    </li>
  );
}

/** The quoted points, capped at `MAX_POINTS`, and a line for how many more the text holds. */
function ConfirmPoints(props: {
  points: SettlePoint[];
  feature: string;
  path: string;
  onNavigate: () => void;
}): JSX.Element {
  const { points, feature, path, onNavigate } = props;
  const shown = points.slice(0, MAX_POINTS);
  return (
    <>
      {shown.length > 0 && (
        <ul className="trace-confirm__points">
          {shown.map((point) => (
            <PointRow
              key={`${point.kind}:${point.index}:${point.quote}`}
              point={point}
              feature={feature}
              path={path}
              onNavigate={onNavigate}
            />
          ))}
        </ul>
      )}
      {points.length > shown.length && (
        <p className="trace-confirm__more">and {points.length - shown.length} more in the text</p>
      )}
    </>
  );
}

function ConfirmActions(props: { onConfirm: () => void; onCancel: () => void }): JSX.Element {
  return (
    <div className="trace-confirm__actions">
      <Button
        type="button"
        size="sm"
        data-look="primary"
        className="plan-button"
        onClick={props.onConfirm}
      >
        Settle anyway
      </Button>
      <Button
        type="button"
        size="sm"
        data-look="secondary"
        className="plan-button"
        onClick={props.onCancel}
      >
        Not yet
      </Button>
    </div>
  );
}

/**
 * The answer to an `unresolved` settle: the counts from the server in words, and each My call and
 * open question quoted from the document text the page already holds (counts alone while the text
 * is not loaded). Settling anyway resends with `confirm=1`.
 */
export function SettleConfirm(props: {
  feature: string;
  path: string;
  myCalls: number;
  openQuestions: number;
  onConfirm: () => void;
  onCancel: () => void;
}): JSX.Element {
  const { feature, path, myCalls, openQuestions, onConfirm, onCancel } = props;
  const { doc } = useKeptDoc(path);
  const points = doc ? settlePoints(doc) : [];
  return (
    <div className="trace-confirm">
      <p className="trace-confirm__counts">{pointsText(myCalls, openQuestions)} still open.</p>
      <p className="trace-confirm__lede">
        Settling says you agree with the document as it stands. These stay in the text; settle again
        once they are resolved.
      </p>
      <ConfirmPoints points={points} feature={feature} path={path} onNavigate={onCancel} />
      <ConfirmActions onConfirm={onConfirm} onCancel={onCancel} />
    </div>
  );
}
