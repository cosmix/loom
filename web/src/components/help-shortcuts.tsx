import type { CSSProperties } from "react";

import { HelpKeys } from "@/components/help-keys";
import type { HelpShortcutGroup } from "@/components/help-topics";
import "./help.css";

// Kept out of help-dialog.tsx so the planning app does not bundle the dashboard's
// terminal and scene modules that help-dialog imports.
export function ShortcutGroups({ groups }: { groups: HelpShortcutGroup[] }) {
  return (
    <div className="help-shortcut-groups">
      {groups.map((group, i) => (
        <section
          key={group.title}
          className="help-shortcut-group help-reveal"
          style={{ "--i": i + 1 } as CSSProperties}
        >
          <h3>{group.title}</h3>
          <dl className="help-shortcuts">
            {group.rows.map(([keys, action]) => (
              <div key={`${keys}${action}`}>
                <dt>
                  <HelpKeys spec={keys} />
                </dt>
                <dd>{action}</dd>
              </div>
            ))}
          </dl>
        </section>
      ))}
    </div>
  );
}
