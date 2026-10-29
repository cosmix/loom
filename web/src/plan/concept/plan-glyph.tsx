import { LinkIcon } from "lucide-react";
import type { JSX } from "react";
import { Link, useLocation } from "react-router";

import { featureOfKey, pageHref, pageKeyOfPath, summaryKey } from "@/plan/api/page-key";
import type { PlanLink, SectionLink } from "@/plan/api/schema-concept";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";

/**
 * The glyph that ends a drilled-into document's first sentence: a popover of the plans that
 * implement a product section, or of the product sections a plan implements (to their summary
 * pages). Nothing renders when there is nothing to list.
 */
export function PlanGlyph({
  plans,
  sections,
}: {
  plans: PlanLink[];
  sections: SectionLink[];
}): JSX.Element | null {
  const { pathname } = useLocation();
  const feature = featureOfKey(pageKeyOfPath(pathname) ?? "");
  if (plans.length === 0 && sections.length === 0) return null;
  const label =
    plans.length > 0 ? "Plans implementing this section" : "Sections this plan implements";
  return (
    <Popover>
      <PopoverTrigger asChild>
        <button type="button" tabIndex={-1} className="concept-plan-glyph" aria-label={label}>
          <LinkIcon className="size-3" aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <PopoverContent aria-label={label} align="start">
        <ul className="m-0 flex list-none flex-col gap-1 p-0 text-[13px]">
          {plans.map((plan) => (
            <li key={plan.slug}>
              <Link
                className="underline"
                to={pageHref(summaryKey(feature, "engineering", plan.slug))}
              >
                {plan.title}
              </Link>
              <span className="ml-2 text-muted-foreground">{plan.state}</span>
            </li>
          ))}
          {sections.map((section) => (
            <li key={section.name}>
              {section.doc ? (
                <Link className="underline" to={pageHref(section.doc.page)}>
                  {section.doc.title}
                </Link>
              ) : (
                <span className="text-muted-foreground">{section.name}</span>
              )}
            </li>
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
