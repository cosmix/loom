import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";

import { frontierHead } from "./model";
import { useTrace } from "./state";

/**
 * The head of the spine: the trace's top attention item as a sentence, an optional muted second
 * line and one primary button to the place that needs the reader. Nothing when the frontier is
 * empty or the trace is not loaded. The button is a link in the primary look
 * (`OfferButton`'s `look="primary"`, as the home page's invitation draws it).
 */
export function SpineHead({ feature }: { feature: string }) {
  const { trace } = useTrace(feature);
  const head = trace ? frontierHead(trace, feature) : null;
  if (!head) return null;
  return (
    <div className="plan-spine-next">
      <p className="plan-spine-next__sentence">
        <Untrusted text={head.sentence} />
      </p>
      {head.detail !== null && (
        <p className="plan-spine-next__detail plan-fine">
          <Untrusted text={head.detail} />
        </p>
      )}
      <Link to={head.to} data-look="primary" className="plan-button plan-spine-next__action">
        <Untrusted text={head.label} />
      </Link>
    </div>
  );
}
