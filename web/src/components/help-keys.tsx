import { MousePointerClickIcon } from "lucide-react";
import { Fragment } from "react";

import { Kbd } from "@/components/ui/kbd";

/// Key names a shortcut row may spell; any other alternative is a pointer gesture.
const KEY_NAMES = new Set([
  "?",
  "/",
  "Esc",
  "Escape",
  "Tab",
  "Shift",
  "Enter",
  "Space",
  "↓",
  "↑",
  "→",
  "←",
]);

/// A key: a named key or a single lowercase letter.
const isKey = (token: string) => KEY_NAMES.has(token) || /^[a-z]$/.test(token);

/// A space-separated sequence of keys (`g p`), pressed one after another.
const sequenceOf = (alternative: string): string[] | null => {
  const keys = alternative.split(" ");
  return keys.length > 1 && keys.every(isKey) ? keys : null;
};

/// Renders a shortcut spec such as `Tab / Shift + Tab` as keycaps: ` / `
/// separates alternatives and ` + ` joins a chord.
export function HelpKeys({ spec }: { spec: string }) {
  return (
    <span className="help-keys">
      {spec.split(" / ").map((alternative, i) => {
        const chord = alternative.split(" + ");
        const sequence = sequenceOf(alternative);
        return (
          <Fragment key={alternative}>
            {i > 0 && <span className="help-keys-or">or</span>}
            {sequence ? (
              <span className="help-chord">
                {sequence.map((key, j) => (
                  <Fragment key={`${j}-${key}`}>
                    {j > 0 && <span className="help-keys-or">then</span>}
                    <Kbd>{key}</Kbd>
                  </Fragment>
                ))}
              </span>
            ) : chord.every(isKey) ? (
              <span className="help-chord">
                {chord.map((key, j) => (
                  <Fragment key={key}>
                    {j > 0 && <span aria-hidden="true">+</span>}
                    <Kbd>{key}</Kbd>
                  </Fragment>
                ))}
              </span>
            ) : (
              <span className="help-gesture">
                <MousePointerClickIcon aria-hidden="true" />
                {alternative}
              </span>
            )}
          </Fragment>
        );
      })}
    </span>
  );
}
