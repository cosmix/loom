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
