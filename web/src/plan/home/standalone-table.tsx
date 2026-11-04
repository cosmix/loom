import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";
import { useId, useMemo, useState, type ReactNode } from "react";
import { Link } from "react-router";

import type { StandalonePlanView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { IsolatedSentence } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import { PLAN_STATE_GLYPHS } from "@/plan/ui/glyphs";

type PlanState = StandalonePlanView["state"];
type SortKey = "title" | "state";
type Sort = { key: SortKey; direction: "ascending" | "descending" } | null;

/** A filter field appears from this many plans; below it the whole list reads at a glance. */
export const FILTER_FROM = 10;

// Work in progress first, then what is ready, then what is finished or set aside.
const STATE_ORDER: Record<PlanState, number> = { running: 0, written: 1, done: 2, retired: 3 };
const STATE_TONE: Record<PlanState, string | undefined> = {
  running: "tone-executing",
  written: undefined,
  done: "tone-completed",
  retired: "tone-dimmed",
};
const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

function shownPlans(plans: StandalonePlanView[], query: string, sort: Sort) {
  const needle = query.trim().toLowerCase();
  const matching = needle
    ? plans.filter(
        (plan) =>
          plan.title.toLowerCase().includes(needle) || plan.slug.toLowerCase().includes(needle),
      )
    : plans;
  if (sort === null) return matching;
  const sign = sort.direction === "ascending" ? 1 : -1;
  return [...matching].sort((a, b) => {
    const byState = STATE_ORDER[a.state] - STATE_ORDER[b.state];
    const byTitle = collator.compare(a.title, b.title);
    return sign * (sort.key === "state" ? byState || byTitle : byTitle || byState);
  });
}

function SortHeader({
  label,
  column,
  sort,
  onSort,
}: {
  label: string;
  column: SortKey;
  sort: Sort;
  onSort: (sort: Sort) => void;
}) {
  const direction = sort?.key === column ? sort.direction : undefined;
  const Icon =
    direction === "ascending" ? ArrowUp : direction === "descending" ? ArrowDown : ChevronsUpDown;
  return (
    <th scope="col" aria-sort={direction} className={`shelf-table__${column}`}>
      <button
        type="button"
        className="shelf-table__sort"
        data-sorted={direction !== undefined || undefined}
        onClick={() =>
          onSort({ key: column, direction: direction === "ascending" ? "descending" : "ascending" })
        }
      >
        {label}
        <Icon aria-hidden="true" />
      </button>
    </th>
  );
}

function PlanRow({
  plan,
  checked,
  selectLabel,
  onToggle,
}: {
  plan: StandalonePlanView;
  checked: boolean;
  selectLabel: string;
  onToggle: (path: string) => void;
}) {
  const titleId = useId();
  const Glyph = PLAN_STATE_GLYPHS[plan.state];
  return (
    <tr data-selected={checked || undefined}>
      <td className="shelf-table__select">
        {/* Named "Select <title>": the title can carry inline markup, so it is referenced, not
            copied into an attribute. */}
        <input
          type="checkbox"
          checked={checked}
          aria-labelledby={`${selectLabel} ${titleId}`}
          onChange={() => onToggle(plan.path)}
        />
      </td>
      <td className="shelf-table__title">
        <div className="shelf-table__plan">
          <Link to={toAddress(plan.path)} className="shelf-table__link">
            <span id={titleId}>
              <IsolatedSentence text={plan.title} />
            </span>
          </Link>
          <span className="shelf-table__slug">
            <Untrusted text={plan.slug} />
          </span>
        </div>
      </td>
      <td className="shelf-table__state">
        <span className="plan-status">
          <Glyph aria-hidden="true" className={STATE_TONE[plan.state]} />
          <Untrusted text={plan.state} />
        </span>
      </td>
    </tr>
  );
}

function SelectAll({
  plans,
  selected,
  onChange,
}: {
  plans: StandalonePlanView[];
  selected: ReadonlySet<string>;
  onChange: (paths: string[], on: boolean) => void;
}) {
  const count = plans.filter((plan) => selected.has(plan.path)).length;
  const all = plans.length > 0 && count === plans.length;
  return (
    <input
      type="checkbox"
      aria-label="Select all shown plans"
      checked={all}
      disabled={plans.length === 0}
      ref={(input) => {
        if (input) input.indeterminate = count > 0 && !all;
      }}
      onChange={() =>
        onChange(
          plans.map((plan) => plan.path),
          !all,
        )
      }
    />
  );
}

export interface StandaloneTableProps {
  plans: StandalonePlanView[];
  selected: ReadonlySet<string>;
  onToggle: (path: string) => void;
  onSelect: (paths: string[], on: boolean) => void;
  /** The selection bar, set on the filter's line at the table's top. */
  actions: ReactNode;
}

/**
 * Every standalone plan, uncapped: a select box, the title (the link) over its slug, and the
 * state as a glyph and a word. Headers sort; from FILTER_FROM plans a field narrows by title or
 * slug. Selection survives filtering, and the header box acts on the plans shown.
 */
export function StandaloneTable({
  plans,
  selected,
  onToggle,
  onSelect,
  actions,
}: StandaloneTableProps) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>(null);
  const selectLabel = useId();
  // The query applies only while its field is shown, so a list that shrinks below FILTER_FROM
  // never keeps rows hidden behind a field that is gone.
  const filtering = plans.length >= FILTER_FROM;
  const activeQuery = filtering ? query : "";
  const shown = useMemo(() => shownPlans(plans, activeQuery, sort), [plans, activeQuery, sort]);
  return (
    <div className="shelf-table-wrap">
      <Toolbar filtering={filtering} query={query} onQuery={setQuery} actions={actions} />
      <span id={selectLabel} hidden>
        Select
      </span>
      <table className="shelf-table">
        <TableHead
          shown={shown}
          selected={selected}
          onSelect={onSelect}
          sort={sort}
          onSort={setSort}
        />
        <TableBody
          shown={shown}
          selected={selected}
          selectLabel={selectLabel}
          query={activeQuery}
          onToggle={onToggle}
        />
      </table>
    </div>
  );
}

function Toolbar({
  filtering,
  query,
  onQuery,
  actions,
}: {
  filtering: boolean;
  query: string;
  onQuery: (query: string) => void;
  actions: ReactNode;
}) {
  return (
    <div className="shelf-toolbar">
      {filtering && (
        <input
          type="search"
          className="shelf-filter"
          aria-label="Filter plans"
          placeholder="Filter by title or slug"
          value={query}
          onChange={(event) => onQuery(event.target.value)}
        />
      )}
      {actions}
    </div>
  );
}

function TableHead({
  shown,
  selected,
  onSelect,
  sort,
  onSort,
}: {
  shown: StandalonePlanView[];
  selected: ReadonlySet<string>;
  onSelect: (paths: string[], on: boolean) => void;
  sort: Sort;
  onSort: (sort: Sort) => void;
}) {
  return (
    <thead>
      <tr>
        <th scope="col" className="shelf-table__select">
          <SelectAll plans={shown} selected={selected} onChange={onSelect} />
        </th>
        <SortHeader label="Plan" column="title" sort={sort} onSort={onSort} />
        <SortHeader label="State" column="state" sort={sort} onSort={onSort} />
      </tr>
    </thead>
  );
}

function TableBody({
  shown,
  selected,
  selectLabel,
  query,
  onToggle,
}: {
  shown: StandalonePlanView[];
  selected: ReadonlySet<string>;
  selectLabel: string;
  query: string;
  onToggle: (path: string) => void;
}) {
  return (
    <tbody>
      {shown.map((plan) => (
        <PlanRow
          key={plan.path}
          plan={plan}
          checked={selected.has(plan.path)}
          selectLabel={selectLabel}
          onToggle={onToggle}
        />
      ))}
      {shown.length === 0 && (
        <tr>
          <td colSpan={3} className="plan-empty shelf-table__none">
            No plan matches “<Untrusted text={query.trim()} />
            ”.
          </td>
        </tr>
      )}
    </tbody>
  );
}
