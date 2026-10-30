import type { ItemView } from "@/plan/api/schema";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { OfferButton } from "@/plan/offers/offer-button";

/**
 * The selected step's checklist items. The step is named by the page title, so
 * rows carry no "Step N" prefix; items a queue row already shows are left out.
 */
export function StepItems({ items }: { items: ItemView[] }) {
  if (items.length === 0) return null;
  return (
    <ul aria-label="Documents and gaps" className="plan-items">
      {items.map((item, index) => (
        <li key={`${index}:${item.text}`} className="plan-items__row">
          <span className="plan-items__label" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
            <SentenceText text={visibleControls(item.text)} />
          </span>
          {item.path && (
            <code className="plan-items__path">
              <Untrusted text={item.path} />
            </code>
          )}
          {item.offer && (
            <div className="plan-items__offer">
              <OfferButton offer={item.offer} />
            </div>
          )}
        </li>
      ))}
    </ul>
  );
}
