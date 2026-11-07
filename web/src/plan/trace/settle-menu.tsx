import { useId, type JSX } from "react";

import { Button } from "@/components/ui/button";

function Item(props: { label: string; note: string; onChoose: () => void }): JSX.Element {
  const noteId = useId();
  return (
    <li className="trace-menu__item">
      <Button
        type="button"
        size="sm"
        data-look="quiet"
        className="plan-button"
        aria-describedby={noteId}
        onClick={props.onChoose}
      >
        {props.label}
      </Button>
      <p id={noteId} className="trace-menu__note">
        {props.note}
      </p>
    </li>
  );
}

/**
 * What a settled or moved document offers from the settle button: settle the text as it reads now
 * (a moved document only) and take the settled mark off. Each choice has a one-line note; the
 * note describes the button, so the button's own name stays the verb.
 */
export function SettleMenu(props: {
  moved: boolean;
  onSettle: () => void;
  onUnsettle: () => void;
}): JSX.Element {
  return (
    <ul className="trace-menu">
      {props.moved && (
        <Item
          label="Settle this version"
          note="Records the text as it reads now as settled."
          onChoose={props.onSettle}
        />
      )}
      <Item
        label="Unsettle"
        note="Makes it a draft again. The text stays as it is."
        onChoose={props.onUnsettle}
      />
    </ul>
  );
}
