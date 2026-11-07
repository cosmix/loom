import { useId, useMemo, useRef, useState, type ReactNode } from "react";
import { Link, useNavigate } from "react-router";

import type { TraceView } from "@/plan/api/schema-trace";
import { Untrusted } from "@/plan/chat/visible-controls";
import { PlanPage } from "@/plan/feature/plan-page";
import { plural } from "@/plan/ui/plural";

import { handleCoverageKey } from "./coverage-keys";
import {
  buildCoverage,
  COLUMNS,
  FILTERS,
  lineage,
  visibleKeys,
  type Card,
  type ColumnId,
  type CoverageModel,
  type Filter,
} from "./coverage-model";
import { TraceGate } from "./trace-gate";

import "./maps.css";

// `/f/<feature>/coverage`: the vision rows, spec headings, metaplan entries, plans and findings
// side by side. Selecting a card marks what it descends from and what follows from it, and opens
// the focus panel. The look is `maps.css`; this file writes the structure and the data attributes
// it reads (`data-state`, `data-lineage`, `data-column`).

type Mark = "self" | "up" | "down";

interface CardView {
  card: Card;
  /** The id of the element holding the card's accessible description. */
  descriptionId: string;
  tabbable: boolean;
  mark: Mark | undefined;
}

function CardButton(props: {
  view: CardView;
  onSelect: (key: string) => void;
  register: (key: string, element: HTMLButtonElement | null) => void;
}) {
  const { view, onSelect, register } = props;
  const { card } = view;
  return (
    <li className="maps-card-item">
      <button
        type="button"
        className="maps-card"
        data-key={card.key}
        ref={(element) => register(card.key, element)}
        tabIndex={view.tabbable ? 0 : -1}
        data-state={card.state}
        data-lineage={view.mark}
        aria-current={view.mark === "self" ? "true" : undefined}
        aria-describedby={card.description === null ? undefined : view.descriptionId}
        onClick={() => onSelect(card.key)}
      >
        <Untrusted text={card.title} />
      </button>
      {card.description !== null && (
        <span id={view.descriptionId} className="sr-only">
          {card.description}
        </span>
      )}
    </li>
  );
}

/** The cards of one column; the spec column sets each section's headings under its title. */
function groupsOf(views: CardView[]): { group: string | null; views: CardView[] }[] {
  const groups: { group: string | null; views: CardView[] }[] = [];
  for (const view of views) {
    const last = groups.at(-1);
    if (last && last.group === view.card.group) last.views.push(view);
    else groups.push({ group: view.card.group, views: [view] });
  }
  return groups;
}

/**
 * One run of cards under its section title. Section titles repeat across the spec column, so the
 * list is named by its title: a screen reader hears which section a card belongs to.
 */
function CardGroup(props: {
  group: string | null;
  views: CardView[];
  onSelect: (key: string) => void;
  register: (key: string, element: HTMLButtonElement | null) => void;
}) {
  const { group, views, onSelect, register } = props;
  const titleId = useId();
  return (
    <div className="maps-group">
      {group !== null && (
        <h3 id={titleId} className="plan-fine maps-group-title">
          <Untrusted text={group} />
        </h3>
      )}
      <ul className="maps-cards" aria-labelledby={group === null ? undefined : titleId}>
        {views.map((view) => (
          <CardButton key={view.card.key} view={view} onSelect={onSelect} register={register} />
        ))}
      </ul>
    </div>
  );
}

function Column(props: {
  id: ColumnId;
  label: string;
  views: CardView[];
  onSelect: (key: string) => void;
  register: (key: string, element: HTMLButtonElement | null) => void;
}) {
  const { id, label, views, onSelect, register } = props;
  const titleId = useId();
  return (
    <section className="maps-column" data-column={id} aria-labelledby={titleId}>
      <h2 id={titleId} className="plan-eyebrow maps-column-title">
        {label}
      </h2>
      {views.length === 0 && <p className="plan-fine maps-empty">None</p>}
      {groupsOf(views).map(({ group, views: inGroup }, index) => (
        <CardGroup
          key={`${index}-${group ?? ""}`}
          group={group}
          views={inGroup}
          onSelect={onSelect}
          register={register}
        />
      ))}
    </section>
  );
}

function Related(props: { heading: string; cards: Card[] }): ReactNode {
  const headingId = useId();
  return (
    <section className="maps-related" aria-labelledby={headingId}>
      <h3 id={headingId} className="plan-eyebrow">
        {props.heading}
      </h3>
      {props.cards.length === 0 ? (
        <p className="plan-fine maps-empty">None</p>
      ) : (
        <ul>
          {props.cards.map((card) => (
            <li key={card.key} data-column={card.column}>
              <Link to={card.href}>
                <Untrusted text={card.title} />
              </Link>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

/** The selected node: what it descends from (Why), what delivers it (How) and what was found. */
function FocusPanel({ model, card }: { model: CoverageModel; card: Card | null }) {
  const { up, down } = card ? lineage(model, card.key) : { up: [], down: [] };
  const cards = (keys: string[]) => keys.map((key) => model.byKey.get(key)!);
  const isFinding = (entry: Card) => entry.column === "findings";
  const titleId = useId();
  return (
    <aside className="maps-focus" aria-labelledby={titleId}>
      <h2 id={titleId} className="maps-focus-title">
        {card ? <Untrusted text={card.title} /> : "Nothing selected"}
      </h2>
      {card === null ? (
        <p className="plan-fine">Select a card to see why it exists and how it is delivered.</p>
      ) : (
        <>
          <Related heading="Why" cards={cards(up).filter((entry) => !isFinding(entry))} />
          <Related heading="How" cards={cards(down).filter((entry) => !isFinding(entry))} />
          <Related heading="Findings" cards={cards([...up, ...down]).filter(isFinding)} />
          <Link className="maps-open" to={card.href}>
            Open
          </Link>
        </>
      )}
    </aside>
  );
}

function FilterBar(props: { filter: Filter; onChange: (filter: Filter) => void }) {
  const name = useId();
  return (
    <fieldset className="maps-filter">
      <legend className="sr-only">Show</legend>
      {FILTERS.map(({ id, label }) => (
        <label key={id} className="maps-filter-option">
          <input
            type="radio"
            name={name}
            checked={props.filter === id}
            onChange={() => props.onChange(id)}
          />
          <span>{label}</span>
        </label>
      ))}
    </fieldset>
  );
}

/** The lineage marks of the selected card, and the card elements that `focus` moves focus to. */
function useCardFocus(model: CoverageModel, current: string | null, select: (key: string) => void) {
  const cardElements = useRef(new Map<string, HTMLButtonElement>());
  const related = useMemo(() => {
    const marks = new Map<string, Mark>();
    if (current === null) return marks;
    const { up, down } = lineage(model, current);
    up.forEach((key) => marks.set(key, "up"));
    down.forEach((key) => marks.set(key, "down"));
    return marks.set(current, "self");
  }, [model, current]);
  const register = (key: string, element: HTMLButtonElement | null) => {
    if (element) cardElements.current.set(key, element);
    else cardElements.current.delete(key);
  };
  const focus = (key: string) => {
    select(key);
    cardElements.current.get(key)?.focus();
  };
  return { related, register, focus };
}

interface ColumnsProps {
  model: CoverageModel;
  visible: Set<string>;
  shown: Card[];
  tabbable: string | null;
  related: Map<string, Mark>;
  focus: (key: string) => void;
  register: (key: string, element: HTMLButtonElement | null) => void;
}

function CoverageColumns({
  model,
  visible,
  shown,
  tabbable,
  related,
  focus,
  register,
}: ColumnsProps) {
  const idBase = useId();
  const navigate = useNavigate();
  const viewOf = (entry: Card): CardView => ({
    card: entry,
    descriptionId: `${idBase}-${model.index.get(entry.key)}`,
    tabbable: entry.key === tabbable,
    mark: related.get(entry.key),
  });
  return (
    // The keys act on the focused card only; the handler ignores events from anywhere else.
    <div
      className="maps-columns"
      onKeyDown={(event) =>
        handleCoverageKey(event, { model, visible, focus, open: (href) => void navigate(href) })
      }
    >
      {COLUMNS.map(({ id, label }) => (
        <Column
          key={id}
          id={id}
          label={label}
          views={shown.filter((entry) => entry.column === id).map(viewOf)}
          onSelect={focus}
          register={register}
        />
      ))}
    </div>
  );
}

function CoverageMap({ trace, feature }: { trace: TraceView; feature: string }) {
  const model = useMemo(() => buildCoverage(trace, feature), [trace, feature]);
  const [filter, setFilter] = useState<Filter>("all");
  const [selected, setSelected] = useState<string | null>(null);
  const visible = useMemo(() => visibleKeys(model, filter), [model, filter]);
  // A card the filter hides is no longer selected.
  const current = selected !== null && visible.has(selected) ? selected : null;
  const { related, register, focus } = useCardFocus(model, current, setSelected);

  const shown = model.cards.filter((entry) => visible.has(entry.key));
  const tabbable = current ?? shown[0]?.key ?? null;

  if (model.cards.length === 0) {
    return (
      <p className="plan-empty">
        Nothing to map yet. Write the vision and its sections first; they are the left of the map.
      </p>
    );
  }
  return (
    <div className="maps-coverage">
      <FilterBar filter={filter} onChange={setFilter} />
      <div className="maps-body">
        <CoverageColumns
          model={model}
          visible={visible}
          shown={shown}
          tabbable={tabbable}
          related={related}
          focus={focus}
          register={register}
        />
        <FocusPanel model={model} card={current === null ? null : model.byKey.get(current)!} />
      </div>
    </div>
  );
}

function lede(trace: TraceView): string {
  const uncovered = trace.headings.filter((heading) => heading.covered_by.length === 0).length;
  return `${plural(trace.headings.length, "spec heading")}, ${uncovered} not covered by any plan.`;
}

/** `/f/:feature/coverage`: the map from vision to plans, with its gaps. */
export function CoveragePage() {
  return (
    <TraceGate noun="the coverage">
      {(trace, feature) => (
        <PlanPage feature={feature} label="coverage" title="Coverage" lede={lede(trace)}>
          <CoverageMap trace={trace} feature={feature} />
        </PlanPage>
      )}
    </TraceGate>
  );
}
