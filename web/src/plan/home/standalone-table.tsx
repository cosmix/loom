import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";
import { useId, useMemo, useState, type ReactNode } from "react";
import { Link } from "react-router";

import type { StandalonePlanView } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
import { IsolatedSentence } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import {
  DEFAULT_SORT,
  firstDirection,
  modifiedLabel,
  shortTitle,
  shownPlans,
  STATE_TONE,
  type Sort,
  type SortKey,
} from "@/plan/home/standalone-order";
import { StateFilter, StateFilterEmpty, useStateFilter } from "@/plan/home/state-filter";
import { PLAN_STATE_WORD_GLYPHS } from "@/plan/ui/glyphs";

import "./standalone-table.css";

/** A filter field appears from this many plans; below it the whole list reads at a glance. */
export const FILTER_FROM = 10;

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
  const direction = sort.key === column ? sort.direction : undefined;
  const Icon =
    direction === "ascending" ? ArrowUp : direction === "descending" ? ArrowDown : ChevronsUpDown;
  const next =
    direction === undefined
      ? firstDirection(column)
      : direction === "ascending"
        ? "descending"
        : "ascending";
  return (
    <th role="columnheader" scope="col" aria-sort={direction} className={`shelf-table__${column}`}>
      <button
        type="button"
        className="shelf-table__sort"
        data-sorted={direction !== undefined || undefined}
        onClick={() => onSort({ key: column, direction: next })}
      >
        {label}
        <Icon aria-hidden="true" />
      </button>
    </th>
  );
}

function Modified({ stamp, now }: { stamp: string | null; now: number }) {
  const label = modifiedLabel(stamp, now);
  if (label === null) return <span className="shelf-table__unknown">unknown</span>;
  return (
    <time dateTime={stamp ?? undefined} title={label.title}>
      {label.text}
    </time>
  );
}

function PlanRow({
  plan,
  checked,
  selectLabel,
  now,
  onToggle,
}: {
  plan: StandalonePlanView;
  checked: boolean;
  selectLabel: string;
  now: number;
  onToggle: (path: string) => void;
}) {
  const titleId = useId();
  const Glyph = PLAN_STATE_WORD_GLYPHS[plan.state];
  return (
    <tr role="row" data-selected={checked || undefined} data-state={plan.state}>
      <td role="cell" className="shelf-table__select">
        {/* Named "Select <title>": the title can carry inline markup, so it is referenced, not
            copied into an attribute. */}
        <input
          type="checkbox"
          checked={checked}
          aria-labelledby={`${selectLabel} ${titleId}`}
          onChange={() => onToggle(plan.path)}
        />
      </td>
      <td role="cell" className="shelf-table__title">
        <div className="shelf-table__plan">
          {/* The link's box covers the row, so the whole row opens the plan. */}
          <Link to={toAddress(plan.path)} className="shelf-table__link">
            <span id={titleId}>
              <IsolatedSentence text={shortTitle(plan.title)} />
            </span>
          </Link>
          <span className="shelf-table__slug">
            <Untrusted text={plan.slug} />
          </span>
        </div>
      </td>
      <td role="cell" className="shelf-table__state">
        <span className="plan-status">
          <Glyph aria-hidden="true" className={STATE_TONE[plan.state]} />
          <Untrusted text={plan.state} />
        </span>
      </td>
      <td role="cell" className="shelf-table__modified">
        <Modified stamp={plan.last_change} now={now} />
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
 * Every standalone plan, uncapped: a select box, the title (the link, covering the row) over its
 * slug, the state as a glyph and a word, and when the plan file last changed. Headers sort,
 * newest first until the reader picks another order; from FILTER_FROM plans a field narrows by
 * title or slug, and with two states or more the state toggles narrow by state (states first,
 * then the field, then the sort). Selection survives the text filter; a plan the state filter
 * hides is unselected (`useStateFilter`). The header box acts on the plans shown.
 */
export function StandaloneTable({
  plans,
  selected,
  onToggle,
  onSelect,
  actions,
}: StandaloneTableProps) {
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>(DEFAULT_SORT);
  // The query applies only while its field is shown, so a list that shrinks below FILTER_FROM
  // never keeps rows hidden behind a field that is gone.
  const filtering = plans.length >= FILTER_FROM;
  const activeQuery = filtering ? query : "";
  const filter = useStateFilter(plans, selected, onSelect);
  const { states } = filter;
  const shown = useMemo(
    () => shownPlans(plans, activeQuery, sort, states),
    [plans, activeQuery, sort, states],
  );
  return (
    <div className="shelf-table-wrap">
      <Toolbar filtering={filtering} query={query} onQuery={setQuery} actions={actions} />
      <StateFilter
        counts={filter.counts}
        total={plans.length}
        picked={states}
        onToggle={filter.toggle}
        onAll={filter.all}
      />
      {shown.length === 0 && states.size > 0 ? (
        <StateFilterEmpty onAll={filter.all} />
      ) : (
        <PlanTable
          shown={shown}
          selected={selected}
          query={activeQuery}
          sort={sort}
          onSort={setSort}
          onSelect={onSelect}
          onToggle={onToggle}
        />
      )}
    </div>
  );
}

/** The table itself: the sortable header and a row per shown plan. */
function PlanTable({
  shown,
  selected,
  query,
  sort,
  onSort,
  onSelect,
  onToggle,
}: {
  shown: StandalonePlanView[];
  selected: ReadonlySet<string>;
  query: string;
  sort: Sort;
  onSort: (sort: Sort) => void;
  onSelect: (paths: string[], on: boolean) => void;
  onToggle: (path: string) => void;
}) {
  const selectLabel = useId();
  // One clock per mount: every row's relative date is measured from the same moment.
  const [now] = useState(() => Date.now());
  return (
    <>
      <span id={selectLabel} hidden>
        Select
      </span>
      {/* The phone layout sets display:block on the table parts, which WebKit reads as dropping their
          semantics; the explicit roles keep them. */}
      <table role="table" className="shelf-table" data-sort={sort.key}>
        <TableHead
          shown={shown}
          selected={selected}
          onSelect={onSelect}
          sort={sort}
          onSort={onSort}
        />
        <TableBody
          shown={shown}
          selected={selected}
          selectLabel={selectLabel}
          query={query}
          now={now}
          onToggle={onToggle}
        />
      </table>
    </>
  );
}

function TableBody({
  shown,
  selected,
  selectLabel,
  query,
  now,
  onToggle,
}: {
  shown: StandalonePlanView[];
  selected: ReadonlySet<string>;
  selectLabel: string;
  query: string;
  now: number;
  onToggle: (path: string) => void;
}) {
  return (
    <tbody role="rowgroup">
      {shown.map((plan) => (
        <PlanRow
          key={plan.path}
          plan={plan}
          checked={selected.has(plan.path)}
          selectLabel={selectLabel}
          now={now}
          onToggle={onToggle}
        />
      ))}
      {shown.length === 0 && (
        <tr role="row">
          <td role="cell" colSpan={4} className="plan-empty shelf-table__none">
            No plan matches “<Untrusted text={query.trim()} />
            ”.
          </td>
        </tr>
      )}
    </tbody>
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
    <thead role="rowgroup">
      <tr role="row">
        <th role="columnheader" scope="col" className="shelf-table__select">
          <SelectAll plans={shown} selected={selected} onChange={onSelect} />
        </th>
        <SortHeader label="Plan" column="title" sort={sort} onSort={onSort} />
        <SortHeader label="State" column="state" sort={sort} onSort={onSort} />
        <SortHeader label="Modified" column="modified" sort={sort} onSort={onSort} />
      </tr>
    </thead>
  );
}
