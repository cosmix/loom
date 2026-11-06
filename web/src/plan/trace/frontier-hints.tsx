import { Link } from "react-router";

import { Untrusted } from "@/plan/chat/visible-controls";

import { frontierLinks } from "./model";
import { useTrace } from "./state";

import "./trace.css";

/**
 * What the feature's trace says needs attention, as at most five links at the head of the spine:
 * documents that moved since they were settled, plans to revisit, drafts to read, spec headings
 * no plan covers and the open findings. They are offers to follow; nothing starts by itself.
 */
export function FrontierHints({ feature }: { feature: string }) {
  const { trace } = useTrace(feature);
  const links = trace ? frontierLinks(trace, feature) : [];
  if (links.length === 0) return null;
  return (
    <ul className="trace-frontier" aria-label="Needs attention">
      {links.map((link) => (
        <li key={`${link.to}|${link.text}`}>
          <Link to={link.to}>
            <Untrusted text={link.text} />
          </Link>
        </li>
      ))}
    </ul>
  );
}
