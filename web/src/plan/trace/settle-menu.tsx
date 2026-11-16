import type { JSX } from "react";

import { StateMenuItem, StateMenuList } from "./state-menu";

/**
 * What a settled or moved document offers from the settle button: settle the text as it reads now
 * (a moved document only) and take the settled mark off. Each choice has a one-line note
 * (`StateMenuItem`).
 */
export function SettleMenu(props: {
  moved: boolean;
  onSettle: () => void;
  onUnsettle: () => void;
}): JSX.Element {
  return (
    <StateMenuList>
      {props.moved && (
        <StateMenuItem
          label="Settle this version"
          note="Records the text as it reads now as settled."
          onChoose={props.onSettle}
        />
      )}
      <StateMenuItem
        label="Unsettle"
        note="Makes it a draft again. The text stays as it is."
        onChoose={props.onUnsettle}
      />
    </StateMenuList>
  );
}
