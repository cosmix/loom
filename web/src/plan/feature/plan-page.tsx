import type { ReactNode } from "react";

import { Untrusted } from "@/plan/chat/visible-controls";

import "./pages.css";

/** The head of a feature page: the eyebrow, the page title and an optional sentence under it. */
export function PageHead({
  eyebrow,
  title,
  lede,
}: {
  eyebrow: string;
  title: string;
  lede?: ReactNode;
}) {
  return (
    <header className="plan-page__head">
      <p className="plan-eyebrow">
        <Untrusted text={eyebrow} />
      </p>
      <h1 className="plan-page-title">{title}</h1>
      {lede && <p className="plan-page__lede">{lede}</p>}
    </header>
  );
}

/**
 * The frame of a feature's own pages (Coverage, Findings): the feature slug and page label as the
 * eyebrow, the page title, and an optional sentence under it, then the page's content.
 */
export function PlanPage({
  feature,
  label,
  title,
  lede,
  children,
}: {
  feature: string;
  label: string;
  title: string;
  lede?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="plan-page">
      <PageHead eyebrow={`${feature} · ${label}`} title={title} lede={lede} />
      {children}
    </div>
  );
}
