import { MousePointerClickIcon } from "lucide-react";
import { Fragment } from "react";

import { Kbd } from "@/components/ui/kbd";

/// Key names a shortcut row may spell; any other alternative is a pointer gesture.
const KEY_NAMES = new Set(["?", "/", "Esc", "Tab", "Shift", "Enter", "Space"]);

/// Renders a shortcut spec such as `Tab / Shift + Tab` as keycaps: ` / `
/// separates alternatives and ` + ` joins a chord.
export function HelpKeys({ spec }: { spec: string }) {
  return (
    <span className="help-keys">
      {spec.split(" / ").map((alternative, i) => {
        const chord = alternative.split(" + ");
        return (
          <Fragment key={alternative}>
            {i > 0 && <span className="help-keys-or">or</span>}
            {chord.every((key) => KEY_NAMES.has(key)) ? (
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
